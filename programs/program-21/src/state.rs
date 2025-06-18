use anchor_lang::prelude::*;
use crate::constants::*; // Импортируем константы для использования (например, MAX_CARDS_IN_HAND)
use std::fmt; // Для реализации Display для Card

// --- Enums ---

/// Масть карты
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Suit {
    #[default]
    Hearts,   // Червы
    Diamonds, // Бубны
    Clubs,    // Трефы
    Spades,   // Пики
}

/// Ранг карты
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Rank {
    Two, Three, Four, Five, Six, Seven, Eight, Nine, Ten,
    Jack,  // Валет
    Queen, // Дама
    King,  // Король
    #[default]
    Ace,   // Туз
}

/// Состояние игрового стола (сессии)
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum GameState {
    AcceptingBets,  // Идет прием ставок
    PlayerTurns,    // Ходы игроков
    DealerTurn,     // Ход дилера
    RoundOver,      // Раунд завершен, можно начинать новый
}

impl Default for GameState {
    fn default() -> Self {
        Self::AcceptingBets
    }
}

/// Статус руки игрока или дилера
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum HandStatus {
    #[default]
    Playing,          // В игре (можно брать карты, стоять, и т.д.)
    Stood,            // Игрок/дилер остановился
    Busted,           // Перебор (больше 21)
    Blackjack,        // Блэкджек (Туз + 10-очковая карта на первых двух картах)
    DoubledAndStood,  // Игрок удвоил ставку, получил одну карту и его ход на этой руке завершен
}


// --- Structs ---

/// Представление игральной карты
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq)]
pub struct Card {
    pub suit: Suit,
    pub rank: Rank,
}

impl Card {
    /// Возвращает значение карты для Блэкджека.
    /// Туз по умолчанию считается за 11.
    pub fn default_value(&self) -> u8 {
        match self.rank {
            Rank::Two => 2,
            Rank::Three => 3,
            Rank::Four => 4,
            Rank::Five => 5,
            Rank::Six => 6,
            Rank::Seven => 7,
            Rank::Eight => 8,
            Rank::Nine => 9,
            Rank::Ten | Rank::Jack | Rank::Queen | Rank::King => 10,
            Rank::Ace => 11, // Туз по умолчанию 11
        }
    }

    /// Проверяет, является ли карта Тузом.
    pub fn is_ace(&self) -> bool {
        self.rank == Rank::Ace
    }

    /// Проверяет, является ли карта "картинкой" (Валет, Дама, Король).
    #[allow(dead_code)] // Может понадобиться для специфических правил или логов
    pub fn is_face_card(&self) -> bool {
        matches!(self.rank, Rank::Jack | Rank::Queen | Rank::King)
    }
}

/// Для отладочного вывода карт (например, "AH" - Туз Червей, "TD" - Десятка Бубен)
impl fmt::Display for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rank_str = match self.rank {
            Rank::Ace => "A", Rank::King => "K", Rank::Queen => "Q", Rank::Jack => "J",
            Rank::Ten => "T", Rank::Nine => "9", Rank::Eight => "8", Rank::Seven => "7",
            Rank::Six => "6", Rank::Five => "5", Rank::Four => "4", Rank::Three => "3",
            Rank::Two => "2",
        };
        let suit_char = match self.suit {
            Suit::Hearts => 'H', // Используем латиницу для простоты в логах
            Suit::Diamonds => 'D',
            Suit::Clubs => 'C',
            Suit::Spades => 'S',
        };
        write!(f, "{}{}", rank_str, suit_char)
    }
}


/// Рука карт игрока или дилера
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, Default)]
pub struct Hand {
    pub cards: Vec<Card>,                 // Карты в руке. Макс. ~11, но Vec для гибкости.
    pub status: HandStatus,               // Текущий статус руки (Playing, Stood, Busted, Blackjack).
    pub bet_multiplier_x100: u16,         // Множитель ставки, умноженный на 100 (например, 100 для 1.0x, 200 для 2.0x после удвоения).
    pub token_mint_for_bet: Pubkey,       // Минт токена, которым сделана ставка на эту руку.
    pub original_bet_amount: u64,         // Первоначальная сумма ставки на эту руку (в UI единицах токена).
}

impl Hand {
    /// Создает новую пустую руку с заданной ставкой.
    pub fn new(token_mint: Pubkey, bet_amount: u64) -> Self {
        Self {
            cards: Vec::with_capacity(MAX_CARDS_IN_HAND), // Предварительное выделение памяти
            status: HandStatus::Playing,                  // Начальный статус - "в игре"
            bet_multiplier_x100: 100,                     // Начальный множитель 1.0x
            token_mint_for_bet: token_mint,
            original_bet_amount: bet_amount,
        }
    }

    /// Добавляет карту в руку.
    pub fn add_card(&mut self, card: Card) {
        if self.cards.len() < MAX_CARDS_IN_HAND { // Защита от переполнения Vec, если он был бы фиксированным
            self.cards.push(card);
        }
    }

    /// Рассчитывает сумму очков в руке.
    /// Возвращает кортеж: (сумма очков, является ли сумма "мягкой" (soft)).
    /// "Мягкая" сумма означает, что Туз считается как 11, и его можно пересчитать как 1, если будет перебор.
    pub fn calculate_score(&self) -> (u8, bool) {
        let mut score: u16 = 0; // Используем u16 для промежуточных сумм во избежание переполнения u8
        let mut num_aces = 0;

        for card in &self.cards {
            score += card.default_value() as u16;
            if card.is_ace() {
                num_aces += 1;
            }
        }

        // Корректируем Тузы (с 11 на 1), если текущая сумма > 21
        while score > 21 && num_aces > 0 {
            score -= 10; // Туз теперь считается как 1, а не 11
            num_aces -= 1;
        }
        
        // Определяем, является ли рука "мягкой"
        // Рука "мягкая", если в ней есть Туз, который все еще считается как 11 (т.е. num_aces не равен количеству всех тузов, если их значение было уменьшено).
        // Проще: если есть хотя бы один туз, и если его посчитать как 1, а не 11, то рука все еще "мягкая".
        // Это означает, что `score` была рассчитана с учетом хотя бы одного туза как 11, и `score <= 21`.
        let is_soft = self.cards.iter().any(|c| c.is_ace()) && // Есть хотя бы один туз
                      score <= 21 &&                            // Сумма не превышает 21
                      self.calculate_hard_score() != score;     // И сумма отличается от "жесткой" (где все тузы по 1)
                                                                // Это значит, что хотя бы один туз был посчитан как 11.
        (score as u8, is_soft)
    }
    
    /// Вспомогательный метод: рассчитывает "жесткую" сумму очков (все Тузы считаются как 1).
    fn calculate_hard_score(&self) -> u16 {
        self.cards.iter().map(|card| {
            if card.is_ace() { 1 } else { card.default_value() as u16 }
        }).sum()
    }

    /// Проверяет, является ли рука "блэкджеком" (Туз + 10-очковая карта на первых двух картах).
    pub fn is_blackjack(&self) -> bool {
        self.cards.len() == 2 && self.calculate_score().0 == 21
    }

    /// Проверяет, является ли сумма очков в руке перебором (> 21).
    pub fn is_busted(&self) -> bool {
        self.calculate_score().0 > 21
    }

    /// Обновляет статус руки (Busted, или Stood если 21) после получения новой карты.
    /// Не устанавливает Blackjack, так как Blackjack определяется на начальной раздаче.
    pub fn update_status_after_card_drawn(&mut self) {
        if self.is_busted() {
            self.status = HandStatus::Busted;
        } else if self.calculate_score().0 == 21 {
            // Если 21 очко, и это не начальная раздача (т.к. Blackjack устанавливается отдельно),
            // то игрок автоматически "стоит" (Stood).
            if self.status != HandStatus::Blackjack { // Не перезаписывать Blackjack, если он был установлен
                self.status = HandStatus::Stood;
            }
        }
        // В противном случае статус остается Playing (если не был изменен на Busted или Stood).
    }

    /// Возвращает эффективную сумму ставки на эту руку (с учетом удвоения).
    pub fn get_effective_bet(&self) -> Result<u64> {
        self.original_bet_amount
            .checked_mul(self.bet_multiplier_x100 as u64)
            .ok_or_else(|| error!(crate::errors::TwentyOneError::ArithmeticOverflow))?
            .checked_div(100) // Делим на 100, так как множитель был x100
            .ok_or_else(|| error!(crate::errors::TwentyOneError::ArithmeticOverflow)) // Технически деление на 100 не вызовет underflow если число >0
    }
}


/// Представление места игрока за столом
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, Default)]
pub struct PlayerSeat {
    pub player_pubkey: Option<Pubkey>,    // Pubkey игрока, если место занято, иначе None.
    pub is_active_in_round: bool,         // Участвует ли игрок в текущем раунде (сделал ли ставку).
    pub hands: Vec<Hand>,                 // Руки игрока (обычно одна, две после сплита). Макс. 2 руки.
    
    // Информация о ставке на текущий раунд (до сплита/удвоения).
    pub current_bet_token_mint: Option<Pubkey>, // Минт токена, которым сделана ставка в этом раунде.
    pub current_bet_amount_staked_ui: u64,    // Сумма ставки в UI-единицах токена.
    pub current_bet_usd_value: u64,           // Проверенная и сохраненная стоимость ставки в USD (в наименьших единицах, например центах).
}

impl PlayerSeat {
    /// Сбрасывает состояние места для нового раунда (ставки, руки).
    pub fn reset_for_new_round(&mut self) {
        self.is_active_in_round = false;
        self.hands.clear(); // Очищаем все руки
        self.current_bet_token_mint = None;
        self.current_bet_amount_staked_ui = 0;
        self.current_bet_usd_value = 0;
    }

    /// Находит индекс первой активной руки (со статусом `Playing`).
    /// Используется для определения, какой рукой игрок ходит следующей.
    pub fn get_first_active_hand_index(&self) -> Option<usize> {
        self.hands.iter().position(|h| h.status == HandStatus::Playing)
    }
}

/// Аккаунт-синглтон для хранения конфигурации авторизации.
/// Хранит публичный ключ бэкенда, который имеет право сажать игроков за стол.
#[account]
#[derive(Default)]
pub struct TableAuthorityConfig {
    pub backend_authority: Pubkey,
    pub bump: u8,
}

impl TableAuthorityConfig {
    pub const SEED_PREFIX: &'static [u8] = b"authority_config";
    // 8 (дискриминатор) + 32 (Pubkey) + 1 (bump)
    pub const CALCULATED_LEN: usize = 8 + 32 + 1;
}

/// Структура для отслеживания баланса токенов (например, в профите дилера)
#[derive(Clone, Debug, AnchorSerialize, AnchorDeserialize)]
pub struct TokenBalance {
    pub mint: Pubkey,
    pub amount: u64,
    pub value_usd: u64,           // ← Готовая цена от бэкенда
    pub pyth_feed_index: u8,      // ← Индекс в remaining_accounts
}

// --- Account (Основной аккаунт контракта) ---

/// Аккаунт, представляющий одну игровую сессию (один стол Блэкджека)
#[account]
#[derive(Debug)]
pub struct GameSession {
    // --- Метаданные стола ---
    pub table_name: String,                       // Уникальное, нормализованное имя стола (макс. 32 символа).
    pub dealer: Pubkey,                           // Pubkey пользователя, создавшего и профинансировавшего стол.
    pub dealer_usdc_escrow: Pubkey,               // PDA, хранящий заблокированный USDC дилера.
    pub dealer_locked_usdc_amount: u64,           // Сумма USDC, заблокированная дилером.
    pub min_bet_usd_equivalent: u64,              // Минимальная ставка за столом (в USD центах).
    pub max_bet_usd_equivalent: u64,              // Максимальная ставка за столом (в USD центах).
    pub min_accepted_token_liquidity: u64,   // Мин. ликвидность (в USD) SPL-токена, чтобы он принимался для ставок.

    // --- Состояние игры ---
    pub game_state: GameState,                    // Текущее состояние игры (Прием ставок, Раздача, и т.д.).
    pub deck: Vec<Card>,                          // Перетасованная последовательность из 4 стандартных 52-карточных колод (208 карт).
    pub current_deck_index: u16,                  // Указатель на следующую карту для взятия из `deck`.
    
    // --- Руки и игроки ---
    pub dealer_hand: Hand,                        // Рука дилера.
    pub player_seats: Vec<PlayerSeat>,            // Места игроков (фиксированный размер, основанный на `max_players`).

    // --- Отслеживание прибыли дилера ---
    /// Хранит "виртуальный" баланс токенов, которые дилер выиграл у игроков.
    /// Фактически токены лежат на эскроу-счетах ставок до момента вывода дилером.
    pub dealer_profit_tracker: Vec<TokenBalance>,

    // --- Верификация тасования ---
    pub seed_elements_hash: [u8; 32],             // Хеш элементов, использованных для сида тасования колоды (слот, время, pubkey дилера, nonce).

    // --- Отслеживание текущего хода ---
    pub current_turn_seat_index: Option<u8>,
    pub current_turn_hand_index: Option<u8>,
    pub current_turn_start_timestamp: Option<i64>,
    pub closing_down: bool,
    
    // --- Commit-Reveal для перетасовки ---
    pub next_shuffle_commitment: Option<[u8; 32]>, // Хеш для следующего сида тасования.
    
    // --- Служебные поля PDA ---
    pub bump: u8,
    pub dealer_usdc_escrow_bump: u8,
}

impl GameSession {
    // Расчет максимального размера аккаунта GameSession для `#[account(init, space = ...)]`
    // Это КРИТИЧЕСКИ важно. Должен быть немного больше реального использования, чтобы избежать ошибок.

    // Размер одной карты (Suit (1 байт) + Rank (1 байт))
    pub const CARD_SIZE: usize = std::mem::size_of::<Suit>() + std::mem::size_of::<Rank>(); // = 2 байта

    // Максимальный размер одной руки (Hand)
    pub const HAND_MAX_LEN_FOR_GAME_SESSION: usize =
        (4 + MAX_CARDS_IN_HAND * Self::CARD_SIZE)  // cards: Vec<Card> (4 байта для длины + N * размер карты)
        + std::mem::size_of::<HandStatus>()      // status: HandStatus (1 байт)
        + std::mem::size_of::<u16>()             // bet_multiplier_x100: u16 (2 байта)
        + std::mem::size_of::<Pubkey>()          // token_mint_for_bet: Pubkey (32 байта)
        + std::mem::size_of::<u64>();            // original_bet_amount: u64 (8 байт)
                                                 // Примерно: (4 + 11*2) + 1 + 2 + 32 + 8 = 26 + 1 + 2 + 32 + 8 = 69 байт

    // Размер 1 руки для PlayerSeat:
    pub const HAND_MAX_LEN_FOR_PLAYER_SEAT: usize =
        (Self::CARD_SIZE * MAX_CARDS_IN_HAND) + // cards (Vec<Card>)
        std::mem::size_of::<HandStatus>() +     // status (enum)
        2 +                                     // bet_multiplier_x100 (u16)
        32 +                                    // token_mint_for_bet (Pubkey)
        8;                                      // original_bet_amount (u64)

    // Макс. размер для Vec<Hand> в PlayerSeat (2 руки)
    pub const HANDS_VEC_MAX_LEN_FOR_PLAYER_SEAT: usize = 4 + (Self::HAND_MAX_LEN_FOR_PLAYER_SEAT * MAX_HANDS_PER_PLAYER); // 4 для Vec len

    pub const PLAYER_SEAT_MAX_LEN: usize =
        (1 + 32) +                              // player_pubkey (Option<Pubkey>)
        1 +                                     // is_active_in_round (bool)
        Self::HANDS_VEC_MAX_LEN_FOR_PLAYER_SEAT + // hands (Vec<Hand>)
        (1 + 32) +                              // current_bet_token_mint (Option<Pubkey>)
        8 +                                     // current_bet_amount_staked_ui (u64)
        8;                                      // current_bet_usd_value (u64)

    pub const PLAYER_SEATS_VEC_MAX_LEN: usize = 4 + (Self::PLAYER_SEAT_MAX_LEN * MAX_PLAYERS_LIMIT as usize);

    // Размер 1 элемента в dealer_profit_tracker:
    pub const TOKEN_BALANCE_SIZE: usize = std::mem::size_of::<Pubkey>() + std::mem::size_of::<u64>(); // 32 + 8 = 40 байт
    // Макс. размер для Vec<TokenBalance> (предполагаем не более 10 разных токенов)
    pub const DEALER_PROFIT_TRACKER_VEC_MAX_LEN: usize = 4 + (Self::TOKEN_BALANCE_SIZE * MAX_DIFFERENT_TOKENS_IN_PROFIT);

    pub const DECK_VEC_MAX_LEN: usize = 4 + (Self::CARD_SIZE * TOTAL_CARDS as usize);

    // Общий расчет размера аккаунта GameSession
    pub const CALCULATED_LEN: usize = 
        8 + // Discriminator
        (4 + TABLE_NAME_MAX_LEN) +                  // table_name (String)
        32 +                                        // dealer (Pubkey)
        32 +                                        // dealer_usdc_escrow (Pubkey)
        8 +                                         // dealer_locked_usdc_amount (u64)
        std::mem::size_of::<GameState>() +          // game_state (enum)
        Self::DECK_VEC_MAX_LEN +                    // deck (Vec<Card>)
        2 +                                         // current_deck_index (u16)
        Self::HAND_MAX_LEN_FOR_GAME_SESSION +       // dealer_hand (Hand)
        Self::PLAYER_SEATS_VEC_MAX_LEN +            // player_seats (Vec<PlayerSeat>)
        Self::DEALER_PROFIT_TRACKER_VEC_MAX_LEN +   // dealer_profit_tracker (Vec<TokenBalance>)
        32 +                                        // seed_elements_hash ([u8; 32])
        (1 + 1) +                                   // current_turn_seat_index (Option<u8>)
        (1 + 1) +                                   // current_turn_hand_index (Option<u8>)
        (1 + 8) +                                   // current_turn_start_timestamp (Option<i64>)
        1 +                                         // closing_down (bool)
        (1 + 32) +                                  // next_shuffle_commitment (Option<[u8; 32]>)
        1 +                                         // bump (u8)
        1;                                          // dealer_usdc_escrow_bump (u8)

    /// Метод для получения следующей карты из колоды и продвижения индекса.
    pub fn draw_card(&mut self) -> Result<Card> {
        if self.current_deck_index >= TOTAL_CARDS {
            // Этого не должно происходить, если есть логика своевременной перетасовки.
            msg!("Error: Deck is empty! Current index: {}, Total cards: {}", self.current_deck_index, TOTAL_CARDS);
            return err!(crate::errors::TwentyOneError::DeckEmpty);
        }
        let card = self.deck[self.current_deck_index as usize];
        self.current_deck_index += 1;
        Ok(card)
    }

    /// Сбрасывает колоду (создает новую из стандартных колод) и тасует ее.
    /// Обновляет `current_deck_index` и `seed_elements_hash`.
    pub fn shuffle_deck(&mut self, seed_elements_hash: [u8; 32]) -> Result<()> {
        self.deck = crate::utils::create_standard_shoe(NUM_DECKS); // Создаем новую полную колоду
        crate::utils::fisher_yates_shuffle(&mut self.deck, seed_elements_hash); // Тасуем ее
        self.current_deck_index = 0; // Сбрасываем индекс на начало колоды
        self.seed_elements_hash = seed_elements_hash; // Сохраняем хеш, использованный для тасования
        Ok(())
    }

    /// Находит изменяемую ссылку на место игрока по его Pubkey.
    /// Возвращает кортеж (индекс места, ссылка на PlayerSeat).
    pub fn find_player_seat_mut(&mut self, player_key: &Pubkey) -> Result<(usize, &mut PlayerSeat)> {
        self.player_seats
            .iter_mut()
            .enumerate()
            .find(|(_, seat)| seat.player_pubkey == Some(*player_key))
            .ok_or_else(|| error!(crate::errors::TwentyOneError::CannotFindPlayerSeat))
    }

    /// Находит неизменяемую ссылку на место игрока по его Pubkey.
    #[allow(dead_code)] // Может быть полезен для read-only операций или проверок
    pub fn find_player_seat(&self, player_key: &Pubkey) -> Result<(usize, &PlayerSeat)> {
        self.player_seats
            .iter()
            .enumerate()
            .find(|(_, seat)| seat.player_pubkey == Some(*player_key))
            .ok_or_else(|| error!(crate::errors::TwentyOneError::CannotFindPlayerSeat))
    }
    
    /// Сбрасывает руки и ставки для всех игроков и дилера для начала нового раунда.
    pub fn reset_hands_for_new_round(&mut self) {
        self.dealer_hand = Hand::default(); // Сброс руки дилера (статус, карты и т.д.)
        for seat in self.player_seats.iter_mut() {
            // Сбрасываем состояние только для занятых мест.
            // Если игрок покинул стол, его player_pubkey будет None.
            if seat.player_pubkey.is_some() {
                seat.reset_for_new_round();
            }
        }
        self.current_turn_seat_index = None; // Сбрасываем информацию о текущем ходе
        self.current_turn_hand_index = None;
        self.current_turn_start_timestamp = None;
    }

    /// Проверяет, есть ли за столом активные игроки, сделавшие ставки.
    #[allow(dead_code)] // Может быть полезен для проверок перед некоторыми действиями
    pub fn has_active_players_with_bets(&self) -> bool {
        self.player_seats.iter().any(|seat| seat.is_active_in_round)
    }

    /// Добавляет или обновляет сумму в трекере прибыли дилера для указанного токена.
    pub fn add_dealer_profit(&mut self, token_mint: Pubkey, amount: u64) -> Result<()> {
        if amount == 0 { return Ok(()); } // Не добавлять нулевой профит

        if let Some(balance) = self.dealer_profit_tracker.iter_mut().find(|b| b.mint == token_mint) {
            balance.amount = balance.amount.checked_add(amount)
                .ok_or(crate::errors::TwentyOneError::ArithmeticOverflow)?;
        } else {
            // Если трекер заполнен, то новый тип токена добавить нельзя,
            // так как место под него не было зарезервировано.
            if self.dealer_profit_tracker.len() >= MAX_DIFFERENT_TOKENS_IN_PROFIT {
                 msg!("Dealer profit tracker is full, cannot add new token type.");
                 // Это не фатальная ошибка для игры, но профит по новому токену не будет отслежен.
                 // В идеале, здесь нужно возвращать ошибку, чтобы бэкенд знал о проблеме.
                 // Например, можно добавить в errors.rs: ProfitTrackerFull
                 // и вызывать `return err!(crate::errors::TwentyOneError::ProfitTrackerFull);`
            }
            self.dealer_profit_tracker.push(TokenBalance { 
                mint: token_mint, 
                amount,
                value_usd: 0,           // Заглушка для совместимости
                pyth_feed_index: 0,     // Заглушка для совместимости
            });
        }
        Ok(())
    }

    /// Уменьшает сумму в трекере прибыли дилера для указанного токена (например, после вывода).
    pub fn reduce_dealer_profit(&mut self, token_mint: Pubkey, amount: u64) -> Result<()> {
        if amount == 0 { return Ok(()); }

        if let Some(balance) = self.dealer_profit_tracker.iter_mut().find(|b| b.mint == token_mint) {
            balance.amount = balance.amount.checked_sub(amount)
                .ok_or(crate::errors::TwentyOneError::ArithmeticOverflow)?; // Ошибка, если пытаемся вычесть больше, чем есть
        } else {
            // Это не должно происходить, если логика вывода верна (проверяем, что токен есть перед вызовом).
            return err!(crate::errors::TwentyOneError::TokenMintNotInProfitTracker);
        }
        Ok(())
    }
}

// --- НОВЫЕ СТРУКТУРЫ И ПЕРЕЧИСЛЕНИЯ, ПЕРЕНЕСЕННЫЕ ИЗ LIB.RS ---

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub enum ForcedAction {
    Hit,
    Stand,
    Split,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub enum PlayerActionType {
    Hit,
    Stand,
    DoubleDown,
    Split,
}

#[derive(Clone, Debug, PartialEq, AnchorSerialize, AnchorDeserialize)]
pub enum HandOutcome {
    Win,              // Обычная победа (1:1)
    Loss,             // Проигрыш
    Push,             // Ничья
    BlackjackWin,     // Блэкджек игрока (3:2)
    BlackjackPush,    // Блэкджек у обоих (возврат ставки)
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct PlayerHandResult {
    pub player: Pubkey,
    pub seat_index: u8,
    pub hand_index: u8,
    pub hand_cards: Vec<Card>,
    pub hand_score: u8,
    pub outcome: HandOutcome,
    pub payout: u64, // Total amount transferred to player (bet + profit)
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct InitialPlayerHand {
    pub player: Pubkey,
    pub seat_index: u8,
    pub hand: Vec<Card>,
}

#[derive(Clone, Debug, AnchorSerialize, AnchorDeserialize)]
pub struct Payout {
    pub payout_amount_ui: u64,
    pub payout_token_mint: Pubkey,
    pub current_price_usd: u64,           // ← Готовая цена от бэкенда
    pub player_account_index: u8,         // ← Индекс в remaining_accounts
    pub escrow_account_index: u8,         // ← Индекс в remaining_accounts
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct FinalizeInstruction {
    // --- Информация для верификации состояния ---
    /// Pubkey игрока, которому предназначена эта инструкция.
    pub player: Pubkey,
    /// Индекс места (seat), для которого производится расчет.
    pub seat_index: u8,
    /// Индекс руки (hand) на этом месте (обычно 0, или 0/1 после сплита).
    pub hand_index: u8,
    /// Карты в руке, по мнению бэкенда. Контракт сверит их с реальными.
    pub hand_cards: Vec<Card>,
    /// Исход игры для этой руки, по мнению бэкенда. Контракт пересчитает и сверит.
    pub outcome: HandOutcome,

    // --- Информация для верификации выплаты ---
    /// Сумма выплаты в UI-единицах токена. Включает возврат ставки и выигрыш.
    /// Контракт пересчитает эту сумму на основе курса Pyth и сверит.
    pub payout_amount_ui: u64,
    /// Ожидаемая цена токена в USD (в наименьших единицах), использованная бэкендом для расчета.
    pub expected_price: i64,

    // --- Индексы аккаунтов в `remaining_accounts` ---
    /// Индекс токен-аккаунта игрока, куда будет отправлена выплата.
    pub player_token_account_index: u8,
    /// Индекс escrow-счета (PDA), с которого будет производиться выплата.
    pub escrow_account_index: u8,
    /// Индекс аккаунта с ценой Pyth для токена этой выплаты.
    pub pyth_feed_index: u8,
}