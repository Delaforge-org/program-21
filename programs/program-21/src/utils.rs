use anchor_lang::prelude::*;
use crate::state::{Card, Suit, Rank, GameState, GameSession, HandStatus, Hand, HandOutcome};
use crate::constants::{
    NUM_DECKS, TABLE_NAME_MIN_LEN, TABLE_NAME_MAX_LEN, BLACKJACK_PAYOUT_PROFIT_NUMERATOR, BLACKJACK_PAYOUT_PROFIT_DENOMINATOR, MAX_PLAYERS_LIMIT,
};
use crate::errors::TwentyOneError;
use sha2::{Sha256, Digest};
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

/// Нормализует и проверяет имя стола.
pub fn normalize_and_validate_table_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.len() < TABLE_NAME_MIN_LEN { return err!(TwentyOneError::TableNameTooShort); }
    if trimmed.len() > TABLE_NAME_MAX_LEN { return err!(TwentyOneError::TableNameTooLong); }

    let normalized: String = trimmed.to_lowercase();

    if !normalized.chars().all(|c| c.is_ascii_lowercase() || c.is_digit(10) || c == '-') {
        return err!(TwentyOneError::TableNameInvalidChars);
    }
    if normalized.starts_with('-') || normalized.ends_with('-') {
        return err!(TwentyOneError::TableNameInvalidHyphenPlacement);
    }
    if normalized.contains("--") {
        return err!(TwentyOneError::TableNameConsecutiveHyphens);
    }
    Ok(normalized)
}

// Создание стандартной колоды
pub fn create_standard_shoe(num_decks: u8) -> Vec<Card> {
    let mut shoe = Vec::with_capacity((num_decks as u16 * NUM_DECKS as u16) as usize);
    let suits = [Suit::Hearts, Suit::Diamonds, Suit::Clubs, Suit::Spades];
    let ranks = [
        Rank::Two, Rank::Three, Rank::Four, Rank::Five, Rank::Six, Rank::Seven,
        Rank::Eight, Rank::Nine, Rank::Ten, Rank::Jack, Rank::Queen, Rank::King, Rank::Ace,
    ];
    for _ in 0..num_decks {
        for &suit in suits.iter() {
            for &rank in ranks.iter() { shoe.push(Card { suit, rank }); }
        }
    }
    shoe
}

// Тасование Фишера-Йейтса
pub fn fisher_yates_shuffle(deck: &mut Vec<Card>, seed_hash: [u8; 32]) {
    if deck.is_empty() { return; }
    let mut rng = ChaCha8Rng::from_seed(seed_hash);
    deck.shuffle(&mut rng);
}

// Генерация хеша для сида тасования
pub fn generate_shuffle_seed_hash(
    slot: u64,
    timestamp: i64,
    dealer_pubkey: &Pubkey,
    nonce: u64,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(slot.to_le_bytes());
    hasher.update(timestamp.to_le_bytes());
    hasher.update(dealer_pubkey.as_ref());
    hasher.update(nonce.to_le_bytes());
    hasher.finalize().into()
}

// Проверка, является ли игрок дилером стола
pub fn verify_dealer_signer(game_session: &Account<GameSession>, dealer_signer: &Signer) -> Result<()> {
    if game_session.dealer != dealer_signer.key() {
        return err!(TwentyOneError::DealerNotSigner);
    }
    Ok(())
}

// Проверка, является ли игрок тем, кто сидит на указанном месте
pub fn verify_player_at_seat(
    game_session: &GameSession,
    player_account: &AccountInfo,
    seat_index: u8,
) -> Result<()> {
    let seat_idx = seat_index as usize;
    if seat_idx >= game_session.player_seats.len() {
        return err!(TwentyOneError::InvalidSeatIndex);
    }
    match game_session.player_seats[seat_idx].player_pubkey {
        Some(key) if key == player_account.key() => Ok(()),
        _ => err!(TwentyOneError::PlayerNotAtSeatIndex),
    }
}

// Проверка, является ли сейчас ход этого игрока на этом месте и для этой руки
pub fn verify_player_turn_and_hand(
    game_session: &GameSession,
    player_account: &AccountInfo,
    seat_index: u8,
    hand_index: u8,
) -> Result<()> {
    if game_session.game_state != GameState::PlayerTurns { return err!(TwentyOneError::NotPlayerTurnsState); }
    match game_session.current_turn_seat_index {
        Some(current_seat_idx) if current_seat_idx == seat_index => {},
        _ => return err!(TwentyOneError::WrongSeatForTurn),
    }
    
    if Some(hand_index) != game_session.current_turn_hand_index { return err!(TwentyOneError::WrongHandForTurn); }
    verify_player_at_seat(game_session, player_account, seat_index)?;

    let hand = game_session.player_seats[seat_index as usize].hands.get(hand_index as usize)
        .ok_or_else(|| error!(TwentyOneError::InvalidHandIndex))?;

    if hand.status != HandStatus::Playing {
        return err!(TwentyOneError::HandActionOnFinalizedHand);
    }

    Ok(())
}

// Определение следующего игрока/руки или переход к дилеру
pub fn determine_next_player_or_transition_to_dealer(game_session: &mut Account<GameSession>, turn_start_timestamp: i64) -> Result<()> {
    let mut found_next_turn = false;

    if let Some(current_seat_idx_u8) = game_session.current_turn_seat_index {
        
        let current_seat_idx = current_seat_idx_u8 as usize;

        // Ищем следующую активную руку у текущего игрока
        if let Some(next_hand_idx) = game_session.player_seats[current_seat_idx].get_first_active_hand_index() {
             game_session.current_turn_hand_index = Some(next_hand_idx as u8);
             game_session.current_turn_start_timestamp = Some(turn_start_timestamp);
             found_next_turn = true;
        }

        // Если у текущего игрока больше нет рук для игры, ищем следующего игрока
        if !found_next_turn {
            for i in 1..MAX_PLAYERS_LIMIT {
                let next_potential_seat_idx = (current_seat_idx + i as usize) % (MAX_PLAYERS_LIMIT as usize);
                
                if game_session.player_seats[next_potential_seat_idx].is_active_in_round {
                    if let Some(next_hand_to_play_idx) = game_session.player_seats[next_potential_seat_idx].get_first_active_hand_index() {
                        game_session.current_turn_seat_index = Some(next_potential_seat_idx as u8);
                        game_session.current_turn_hand_index = Some(next_hand_to_play_idx as u8);
                        game_session.current_turn_start_timestamp = Some(turn_start_timestamp);
                        found_next_turn = true;
                        break; 
                    }
                }
            }
        }
    } else {
        // Если не нашли никого, переходим к ходу дилера
        game_session.game_state = GameState::DealerTurn;
        game_session.current_turn_seat_index = None;
        game_session.current_turn_hand_index = None;
        game_session.current_turn_start_timestamp = None;
    }

    if !found_next_turn {
        // Если не нашли никого, переходим к ходу дилера
        game_session.game_state = GameState::DealerTurn;
        game_session.current_turn_seat_index = None;
        game_session.current_turn_hand_index = None;
        game_session.current_turn_start_timestamp = None;
    }
    Ok(())
}

/// Рассчитывает ожидаемый возврат средств в USD для одной руки.
pub fn calculate_expected_usd_return(
    hand: &Hand,
    effective_bet_usd: u128,
    dealer_final_score: u8,
    dealer_is_busted: bool,
    dealer_has_blackjack: bool,
) -> Result<(u128, HandOutcome)> {
    let player_final_score = hand.calculate_score().0;

    let result = match hand.status {
        HandStatus::Blackjack => {
            if dealer_has_blackjack {
                (effective_bet_usd, HandOutcome::BlackjackPush) // Пуш, возврат ставки
            } else {
                // Выигрыш Блэкджек. Возврат ставки + профит.
                let profit = (effective_bet_usd * BLACKJACK_PAYOUT_PROFIT_NUMERATOR as u128)
                    .checked_div(BLACKJACK_PAYOUT_PROFIT_DENOMINATOR as u128)
                    .ok_or(TwentyOneError::ArithmeticOverflow)?;
                (effective_bet_usd.checked_add(profit).ok_or(TwentyOneError::ArithmeticOverflow)?, HandOutcome::BlackjackWin)
            }
        },
        HandStatus::Busted => (0, HandOutcome::Loss), // Проигрыш, возврат 0
        HandStatus::Stood | HandStatus::DoubledAndStood => {
            if dealer_is_busted || player_final_score > dealer_final_score {
                // Обычный выигрыш. Возврат ставки + выигрыш (равный ставке). Итого ставка * 2.
                (effective_bet_usd.checked_mul(2).ok_or(TwentyOneError::ArithmeticOverflow)?, HandOutcome::Win)
            } else if player_final_score == dealer_final_score {
                (effective_bet_usd, HandOutcome::Push) // Пуш, возврат ставки
            } else {
                (0, HandOutcome::Loss) // Проигрыш, возврат 0
            }
        },
        _ => return err!(TwentyOneError::HandActionOnFinalizedHand), // Рука не в финальном статусе
    };
    Ok(result)
}