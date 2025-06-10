use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Mint, Transfer, CloseAccount};
use std::str::FromStr;

// Локальные модули
mod constants;
mod errors;
mod state;
mod utils;

// Импорт из локальных модулей для удобства
use constants::*;
use errors::*;
use state::*;
use utils::*;

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS"); // ЗАГЛУШКА ID, замените!

#[program]
pub mod blackjack_anchor {
    use super::*; // Для доступа к items из constants, errors, state, utils

    // --- 3.1. initialize_table ---
    pub fn initialize_table(
        ctx: Context<InitializeTable>,
        table_name_input: String,
        max_players_input: u8,
        min_bet_usd_input: u64, 
        max_bet_usd_input: u64, 
        min_token_cap_usd_input: u64,
        shuffle_seed_nonce: u64,
    ) -> Result<()> {
        let game_session = &mut ctx.accounts.game_session_account;
        let dealer = &ctx.accounts.dealer;
        let clock = &ctx.accounts.clock;

        let normalized_table_name = normalize_and_validate_table_name(&table_name_input)?;
        
        if !(1..=MAX_PLAYERS_LIMIT).contains(&max_players_input) { return err!(BlackjackError::InvalidMaxPlayers); }
        if min_bet_usd_input == 0 { return err!(BlackjackError::MinBetIsZero); }
        if min_bet_usd_input > max_bet_usd_input { return err!(BlackjackError::MinBetExceedsMaxBet); }

        let required_collateral = max_bet_usd_input
            .checked_mul(max_players_input as u64).ok_or(BlackjackError::CollateralOverflow)?
            .checked_mul(2).ok_or(BlackjackError::CollateralOverflow)?; // x2 коэффициент обеспечения
        
        let usdc_mint_pubkey = Pubkey::from_str(USDC_MINT_PUBKEY_STR).map_err(|_| BlackjackError::PubkeyParseError)?;
        if ctx.accounts.dealer_usdc_token_account.mint != usdc_mint_pubkey { return err!(BlackjackError::UsdcMintMismatch); }
        if ctx.accounts.usdc_mint.key() != usdc_mint_pubkey { return err!(BlackjackError::UsdcMintMismatch); }

        token::transfer( CpiContext::new( ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.dealer_usdc_token_account.to_account_info(),
                    to: ctx.accounts.dealer_usdc_escrow_account.to_account_info(),
                    authority: dealer.to_account_info(),
                }),
            required_collateral
        )?;

        game_session.table_name = normalized_table_name;
        game_session.dealer = dealer.key();
        game_session.dealer_usdc_escrow = ctx.accounts.dealer_usdc_escrow_account.key();
        game_session.dealer_locked_usdc_amount = required_collateral;
        game_session.max_players = max_players_input;
        game_session.min_bet_usd_equivalent = min_bet_usd_input;
        game_session.max_bet_usd_equivalent = max_bet_usd_input;
        game_session.min_accepted_token_capitalization = min_token_cap_usd_input;
        game_session.owner_fee_recipient = Pubkey::from_str(DEFAULT_OWNER_FEE_RECIPIENT_STR).map_err(|_| BlackjackError::PubkeyParseError)?;
        game_session.game_state = GameState::AcceptingBets;
        game_session.bets_acceptance_stopped = false;
        
        let seed_hash = generate_shuffle_seed_hash( clock.slot, clock.unix_timestamp, &dealer.key(), shuffle_seed_nonce);
        game_session.shuffle_deck(seed_hash)?; // Использует метод из GameSession impl
        
        game_session.dealer_hand = Hand::default();
        game_session.player_seats = vec![PlayerSeat::default(); max_players_input as usize];
        game_session.dealer_profit_tracker = Vec::new();
        game_session.current_turn_seat_index = None;
        game_session.current_turn_hand_index = None;
        game_session.bump = *ctx.bumps.get("game_session_account").ok_or(ProgramError::SecurityViolation)?; // Anchor 0.29+
        game_session.dealer_usdc_escrow_bump = *ctx.bumps.get("dealer_usdc_escrow_account").ok_or(ProgramError::SecurityViolation)?;

        msg!("Table '{}' initialized by dealer {}. Collateral: {}", game_session.table_name, dealer.key(), required_collateral);
        Ok(())
    }

    // --- 3.2. join_table ---
    pub fn join_table(ctx: Context<JoinTable>, seat_index: u8) -> Result<()> {
        let game_session = &mut ctx.accounts.game_session_account;
        let player_account = &ctx.accounts.player_account;
        let seat_idx = seat_index as usize;

        if seat_idx >= game_session.player_seats.len() { return err!(BlackjackError::InvalidSeatIndex); }
        if game_session.player_seats[seat_idx].player_pubkey.is_some() { return err!(BlackjackError::SeatTaken); }

        game_session.player_seats[seat_idx].player_pubkey = Some(player_account.key());
        game_session.player_seats[seat_idx].reset_for_new_round(); 
        msg!("Player {} joined table {} at seat {}", player_account.key(), game_session.table_name, seat_index);
        Ok(())
    }

    // --- 3.3. leave_table ---
    pub fn leave_table(ctx: Context<LeaveTable>, seat_index: u8) -> Result<()> {
        let game_session = &mut ctx.accounts.game_session_account;
        let player_account = &ctx.accounts.player_account;
        let seat_idx = seat_index as usize;

        verify_player_at_seat(game_session, player_account, seat_index)?; // Проверяет, что игрок на месте
        if game_session.player_seats[seat_idx].is_active_in_round { return err!(BlackjackError::PlayerHasActiveBet); }
        // Дополнительная проверка: нельзя покидать стол, если не AcceptingBets или RoundOver, и руки не пусты
        if (game_session.game_state != GameState::AcceptingBets && game_session.game_state != GameState::RoundOver) &&
           !game_session.player_seats[seat_idx].hands.is_empty() {
            return err!(BlackjackError::PlayerHasActiveBet); // Обобщенная ошибка "нельзя покинуть"
        }
        
        game_session.player_seats[seat_idx].player_pubkey = None;
        game_session.player_seats[seat_idx].reset_for_new_round();
        msg!("Player {} left table {} from seat {}", player_account.key(), game_session.table_name, seat_index);
        Ok(())
    }

    // --- 3.4. place_bet ---
    pub fn place_bet(
        ctx: Context<PlaceBet>,
        seat_index: u8,
        amount_staked_ui: u64, 
        usd_value_of_bet: u64, 
    ) -> Result<()> {
        let game_session = &mut ctx.accounts.game_session_account;
        let player_account = &ctx.accounts.player_account;
        let token_mint_key = ctx.accounts.player_spl_token_account.mint;

        if game_session.game_state != GameState::AcceptingBets { return err!(BlackjackError::NotAcceptingBets); }
        if game_session.bets_acceptance_stopped { return err!(BlackjackError::BetsAcceptanceStoppedByDealer); }

        verify_player_at_seat(game_session, player_account, seat_index)?;
        let seat_idx = seat_index as usize;

        if game_session.player_seats[seat_idx].is_active_in_round { return err!(BlackjackError::PlayerHasActiveBet); }
        if usd_value_of_bet < game_session.min_bet_usd_equivalent { return err!(BlackjackError::BetBelowMin); }
        if usd_value_of_bet > game_session.max_bet_usd_equivalent { return err!(BlackjackError::BetAboveMax); }

        token::transfer( CpiContext::new( ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.player_spl_token_account.to_account_info(),
                    to: ctx.accounts.game_session_spl_escrow_account.to_account_info(),
                    authority: player_account.to_account_info(),
                }),
            amount_staked_ui
        )?;
        
        let player_seat = &mut game_session.player_seats[seat_idx];
        player_seat.current_bet_token_mint = Some(token_mint_key);
        player_seat.current_bet_amount_staked_ui = amount_staked_ui;
        player_seat.current_bet_usd_value = usd_value_of_bet;
        player_seat.is_active_in_round = true;
        player_seat.hands.clear();
        player_seat.hands.push(Hand::new(token_mint_key, amount_staked_ui));

        msg!("Player {} placed bet {} (USD {}) at seat {} with token {}", player_account.key(), amount_staked_ui, usd_value_of_bet, seat_index, token_mint_key);
        Ok(())
    }

    // --- 3.5. deal_initial_cards ---
    pub fn deal_initial_cards(ctx: Context<DealInitialCards>) -> Result<()> {
        let game_session = &mut ctx.accounts.game_session_account;
        verify_dealer_signer(game_session, &ctx.accounts.dealer_account)?;
        
        if game_session.game_state != GameState::AcceptingBets { return err!(BlackjackError::InvalidGameStateForDeal); }
        let active_player_count = game_session.player_seats.iter().filter(|s| s.is_active_in_round).count();
        if active_player_count < MIN_PLAYERS_FOR_DEAL as usize { return err!(BlackjackError::NotEnoughPlayers); }
        
        let cards_needed = (active_player_count * 2) + 2; // 2 карты каждому активному игроку + 2 дилеру
        if (TOTAL_CARDS - game_session.current_deck_index) < cards_needed as u16 { return err!(BlackjackError::DeckEmpty); }

        game_session.game_state = GameState::Dealing;
        msg!("State: Dealing. Table: {}", game_session.table_name);

        for seat_idx in 0..game_session.player_seats.len() {
            if game_session.player_seats[seat_idx].is_active_in_round {
                 if let Some(hand) = game_session.player_seats[seat_idx].hands.get_mut(0) {
                    hand.add_card(game_session.draw_card()?);
                    hand.add_card(game_session.draw_card()?);
                    msg!("Dealt to player {:?} seat {}: {} {}", game_session.player_seats[seat_idx].player_pubkey.unwrap_or_default(), seat_idx, hand.cards[0], hand.cards[1]);
                } else { return Err(ProgramError::SecurityViolation); /* Should not happen */ }
            }
        }
        
        game_session.dealer_hand.cards.clear(); // Рука дилера сбрасывается
        game_session.dealer_hand.token_mint_for_bet = Pubkey::default(); // У дилера нет ставки в токенах
        game_session.dealer_hand.original_bet_amount = 0;
        game_session.dealer_hand.bet_multiplier_x100 = 100;
        game_session.dealer_hand.status = HandStatus::Playing; // Начальный статус
        game_session.dealer_hand.add_card(game_session.draw_card()?);
        game_session.dealer_hand.add_card(game_session.draw_card()?); // Обе карты дилера берутся сразу
        msg!("Dealer's up card: {}", game_session.dealer_hand.cards[0]);

        let dealer_has_blackjack = game_session.dealer_hand.is_blackjack();
        if dealer_has_blackjack { game_session.dealer_hand.status = HandStatus::Blackjack; msg!("Dealer Blackjack!"); }
        
        let mut all_players_have_blackjack_or_resolved = true;
        for seat_idx in 0..game_session.player_seats.len() {
            if game_session.player_seats[seat_idx].is_active_in_round {
                if let Some(hand) = game_session.player_seats[seat_idx].hands.get_mut(0) {
                    if hand.is_blackjack() { 
                        hand.status = HandStatus::Blackjack; 
                        msg!("Player seat {} Blackjack!", seat_idx);
                    } else { 
                        all_players_have_blackjack_or_resolved = false; // Есть игрок без блэкджека
                    }
                }
            }
        }

        if dealer_has_blackjack || (all_players_have_blackjack_or_resolved && active_player_count > 0) {
            game_session.game_state = GameState::RoundOver;
            game_session.current_turn_seat_index = None;
            game_session.current_turn_hand_index = None;
            msg!("Round immediately over due to dealer/all players blackjack. State: RoundOver");
        } else {
            game_session.game_state = GameState::PlayerTurns;
            // Найти первого активного игрока, у которого не блэкджек
            let first_player_to_act_idx = game_session.player_seats.iter().position(
                |s| s.is_active_in_round && s.hands.get(0).map_or(false, |h| h.status != HandStatus::Blackjack)
            ); 
            if let Some(idx) = first_player_to_act_idx {
                game_session.current_turn_seat_index = Some(idx as u8);
                game_session.current_turn_hand_index = Some(0); // Начинаем с первой (и единственной на данный момент) руки
                msg!("State: PlayerTurns. First player to act: seat {}", idx);
            } else {
                // Если не нашли игрока для хода (все активные игроки - блэкджек, но дилер - нет)
                // Этот случай должен был быть отловлен `all_players_have_blackjack_or_resolved`
                // Для безопасности, если все же попали сюда, то это RoundOver.
                game_session.game_state = GameState::RoundOver;
                msg!("No players eligible to act after deal (all active players must have blackjack). State: RoundOver");
            }
        }
        Ok(())
    }
    
    // --- 3.6. player_action_hit ---
    pub fn player_action_hit(ctx: Context<PlayerAction>, seat_index: u8, hand_index: u8) -> Result<()> {
        let game_session = &mut ctx.accounts.game_session_account;
        verify_player_turn_and_hand(game_session, &ctx.accounts.player_account, seat_index, hand_index)?;
        
        let hand = game_session.player_seats[seat_index as usize].hands.get_mut(hand_index as usize)
            .ok_or_else(|| error!(BlackjackError::InvalidHandIndex))?; // Должно быть проверено verify_player_turn_and_hand

        let new_card = game_session.draw_card()?;
        hand.add_card(new_card);
        hand.update_status_after_card_drawn(); // Обновит на Busted или Stood (если 21)
        
        msg!("Player seat {} hand {} hits, gets card: {}. Hand score: {}, status: {:?}", 
            seat_index, hand_index, new_card, hand.calculate_score().0, hand.status);
        
        if hand.status != HandStatus::Playing { // Если рука завершена (Busted/Stood)
            determine_next_player_or_transition_to_dealer(game_session)?;
        }
        // Если Playing, ход остается у этой же руки.
        Ok(())
    }

    // --- 3.7. player_action_stand ---
    pub fn player_action_stand(ctx: Context<PlayerAction>, seat_index: u8, hand_index: u8) -> Result<()> {
        let game_session = &mut ctx.accounts.game_session_account;
        verify_player_turn_and_hand(game_session, &ctx.accounts.player_account, seat_index, hand_index)?;

        let hand = game_session.player_seats[seat_index as usize].hands.get_mut(hand_index as usize)
            .ok_or_else(|| error!(BlackjackError::InvalidHandIndex))?;
        
        hand.status = HandStatus::Stood;
        msg!("Player at seat {} hand {} stands. Score: {}", seat_index, hand_index, hand.calculate_score().0);
        determine_next_player_or_transition_to_dealer(game_session)?;
        Ok(())
    }

    // --- 3.8. player_action_double_down ---
    pub fn player_action_double_down(ctx: Context<PlayerActionDoubleOrSplit>, seat_index: u8, hand_index: u8) -> Result<()> {
        let game_session = &mut ctx.accounts.game_session_account;
        verify_player_turn_and_hand(game_session, &ctx.accounts.player_account, seat_index, hand_index)?;
        
        let player_seat = &mut game_session.player_seats[seat_index as usize];
        // Важно: получаем ссылку на руку *после* проверки токенов, чтобы избежать проблем с borrow checker
        // или клонируем нужные поля для проверки токенов
        let hand_token_mint_for_bet;
        let hand_original_bet_amount;
        let hand_cards_len;
        {
            let hand_check = player_seat.hands.get(hand_index as usize).ok_or(BlackjackError::InvalidHandIndex)?;
             if hand_check.status != HandStatus::Playing { return err!(BlackjackError::HandActionOnFinalizedHand); }
            if hand_check.cards.len() != 2 { return err!(BlackjackError::CannotDoubleNotTwoCards); }
            hand_token_mint_for_bet = hand_check.token_mint_for_bet;
            hand_original_bet_amount = hand_check.original_bet_amount;
            hand_cards_len = hand_check.cards.len();
        }
        if hand_cards_len != 2 { return err!(BlackjackError::CannotDoubleNotTwoCards); } // Повторная проверка для ясности

        if ctx.accounts.player_spl_token_account.mint != hand_token_mint_for_bet { return err!(BlackjackError::BetTokenMintMismatch); }
        if ctx.accounts.game_session_spl_escrow_account.mint != hand_token_mint_for_bet { return err!(BlackjackError::BetTokenMintMismatch); }
        
        let additional_stake = hand_original_bet_amount;
        if ctx.accounts.player_spl_token_account.amount < additional_stake { return err!(BlackjackError::InsufficientFundsForDoubleDown); }

        token::transfer( CpiContext::new( ctx.accounts.token_program.to_account_info(), Transfer {
                from: ctx.accounts.player_spl_token_account.to_account_info(),
                to: ctx.accounts.game_session_spl_escrow_account.to_account_info(),
                authority: ctx.accounts.player_account.to_account_info(),
            }), additional_stake)?;

        // Теперь получаем изменяемую ссылку на руку
        let hand = player_seat.hands.get_mut(hand_index as usize).ok_or(BlackjackError::InvalidHandIndex)?;
        hand.bet_multiplier_x100 = 200; // 2.0x
        let new_card = game_session.draw_card()?;
        hand.add_card(new_card);
        
        hand.update_status_after_card_drawn(); // Проверит на Bust или 21 (Stood)
        // После дабла и одной карты ход всегда завершается, статус становится DoubledAndStood, если не Bust.
        if hand.status != HandStatus::Busted { hand.status = HandStatus::DoubledAndStood; }
        
        msg!("Player seat {} hand {} doubles down, gets card: {}. Score: {}, Status: {:?}", 
            seat_index, hand_index, new_card, hand.calculate_score().0, hand.status);
        determine_next_player_or_transition_to_dealer(game_session)?;
        Ok(())
    }
    
    // --- 3.9. player_action_split ---
    pub fn player_action_split(ctx: Context<PlayerActionDoubleOrSplit>, seat_index: u8, hand_index: u8) -> Result<()> {
        let game_session = &mut ctx.accounts.game_session_account;
        verify_player_turn_and_hand(game_session, &ctx.accounts.player_account, seat_index, hand_index)?;
        // Сплит возможен только для первоначальной руки (индекс 0) и если еще не сплитовали
        if hand_index != 0 { return err!(BlackjackError::CannotSplitAlreadySplit); }

        let seat_idx = seat_index as usize;
        // Клонируем данные для проверки перед изменяемым заимствованием
        let original_hand_cards_clone;
        let original_hand_token_mint;
        let original_hand_bet_amount;
        let hands_len;
        {
            let player_seat_check = &game_session.player_seats[seat_idx];
            hands_len = player_seat_check.hands.len();
            let original_hand_check = player_seat_check.hands.get(0).ok_or(BlackjackError::InvalidHandIndex)?; // hand_index всегда 0 для сплита
            if original_hand_check.status != HandStatus::Playing { return err!(BlackjackError::HandActionOnFinalizedHand); }
            if original_hand_check.cards.len() != 2 { return err!(BlackjackError::CannotSplitNotTwoCards); }
            if original_hand_check.cards[0].default_value() != original_hand_check.cards[1].default_value() { return err!(BlackjackError::CannotSplitRanksMismatch); }
            original_hand_cards_clone = original_hand_check.cards.clone();
            original_hand_token_mint = original_hand_check.token_mint_for_bet;
            original_hand_bet_amount = original_hand_check.original_bet_amount;
        }
        if hands_len != 1 { return err!(BlackjackError::CannotSplitAlreadySplit); } // Проверка, что еще не сплитовали

        if ctx.accounts.player_spl_token_account.mint != original_hand_token_mint { return err!(BlackjackError::BetTokenMintMismatch); }
        if ctx.accounts.game_session_spl_escrow_account.mint != original_hand_token_mint { return err!(BlackjackError::BetTokenMintMismatch); }
        
        let stake_for_new_hand = original_hand_bet_amount;
        if ctx.accounts.player_spl_token_account.amount < stake_for_new_hand { return err!(BlackjackError::InsufficientFundsForSplit); }

        token::transfer( CpiContext::new( ctx.accounts.token_program.to_account_info(), Transfer {
                from: ctx.accounts.player_spl_token_account.to_account_info(),
                to: ctx.accounts.game_session_spl_escrow_account.to_account_info(),
                authority: ctx.accounts.player_account.to_account_info(),
            }), stake_for_new_hand)?;

        // Теперь получаем изменяемый доступ
        let player_seat = &mut game_session.player_seats[seat_idx];
        let card_for_new_hand = player_seat.hands[0].cards.pop().ok_or(ProgramError::SecurityViolation)?; // Забираем вторую карту
        
        let mut new_hand = Hand::new(original_hand_token_mint, stake_for_new_hand);
        new_hand.add_card(card_for_new_hand);
        
        player_seat.hands[0].bet_multiplier_x100 = 100; // Сброс множителя для первой руки
        new_hand.bet_multiplier_x100 = 100;       // Множитель для новой руки
        player_seat.hands.push(new_hand);         // Добавляем новую руку (теперь их 2)

        msg!("Player at seat {} splits. Original hand card: {}. New hand card: {}",
             seat_index, player_seat.hands[0].cards[0], player_seat.hands[1].cards[0]);
             
        // Раздача по одной карте на каждую руку
        for hand_idx_to_deal in 0..player_seat.hands.len() {
             if hand_idx_to_deal > 1 { break; } // Только для первых двух рук после сплита
             let card_dealt = game_session.draw_card()?;
             player_seat.hands[hand_idx_to_deal].add_card(card_dealt);
             msg!("Dealt {} to split hand index {}. Hand: {} {}", card_dealt, hand_idx_to_deal, player_seat.hands[hand_idx_to_deal].cards[0], player_seat.hands[hand_idx_to_deal].cards[1]);

             // Правило: если сплитили тузы, то на каждую руку дается только одна карта, и игра этой рукой завершается.
             if player_seat.hands[hand_idx_to_deal].cards[0].is_ace() { // Первая карта была тузом
                 player_seat.hands[hand_idx_to_deal].status = HandStatus::Stood; // Авто-стенд
                 msg!("Split hand index {} with Ace received second card. Auto-stand. Score: {}", hand_idx_to_deal, player_seat.hands[hand_idx_to_deal].calculate_score().0);
             } else {
                 player_seat.hands[hand_idx_to_deal].update_status_after_card_drawn();
             }
             msg!("Split hand index {} score: {}, status: {:?}", hand_idx_to_deal, player_seat.hands[hand_idx_to_deal].calculate_score().0, player_seat.hands[hand_idx_to_deal].status);
        }
        
        // Игрок продолжает играть первой разделенной рукой (hand_index 0), если она еще Playing.
        game_session.current_turn_hand_index = Some(0); 
        if player_seat.hands[0].status != HandStatus::Playing {
            determine_next_player_or_transition_to_dealer(game_session)?;
        } else {
             msg!("Player continues with first split hand (index 0).");
        }
        Ok(())
    }

    // --- 3.10. dealer_play_turn ---
    pub fn dealer_play_turn(ctx: Context<DealerAction>) -> Result<()> {
        let game_session = &mut ctx.accounts.game_session_account;
        verify_dealer_signer(game_session, &ctx.accounts.dealer_account)?;
        if game_session.game_state != GameState::DealerTurn { return err!(BlackjackError::NotDealerTurnState); }

        msg!("Dealer's turn. Initial hand: {} {} (score: {})",
             game_session.dealer_hand.cards.get(0).copied().unwrap_or_default(), 
             game_session.dealer_hand.cards.get(1).copied().unwrap_or_default(),
             game_session.dealer_hand.calculate_score().0);

        loop {
            let (score, is_soft) = game_session.dealer_hand.calculate_score();
            
            if score > 21 {
                game_session.dealer_hand.status = HandStatus::Busted;
                msg!("Dealer busts with score: {}", score);
                break;
            }
            if score > 17 { // Жесткие 18+ или мягкие 18+
                game_session.dealer_hand.status = HandStatus::Stood;
                msg!("Dealer stands with score: {}", score);
                break;
            }
            if score == 17 {
                if is_soft { // Мягкие 17 (H17 правило - HIT)
                    msg!("Dealer has soft 17, hits.");
                    let new_card = game_session.draw_card()?;
                    game_session.dealer_hand.add_card(new_card);
                    msg!("Dealer draws: {}. New hand: {:?}, New score: {}", new_card, game_session.dealer_hand.cards, game_session.dealer_hand.calculate_score().0);
                } else { // Жесткие 17 (STAND)
                    game_session.dealer_hand.status = HandStatus::Stood;
                    msg!("Dealer stands with hard 17.");
                    break;
                }
            } else { // score < 17, дилер должен брать
                msg!("Dealer score is {}, hits.", score);
                let new_card = game_session.draw_card()?;
                game_session.dealer_hand.add_card(new_card);
                msg!("Dealer draws: {}. New hand: {:?}, New score: {}", new_card, game_session.dealer_hand.cards, game_session.dealer_hand.calculate_score().0);
            }
        }
        
        game_session.game_state = GameState::RoundOver;
        game_session.current_turn_seat_index = None; 
        game_session.current_turn_hand_index = None;
        msg!("Dealer's turn finished. Final score: {}. Status: {:?}. Game state: RoundOver", 
            game_session.dealer_hand.calculate_score().0, game_session.dealer_hand.status);
        Ok(())
    }
    
    // --- 3.11. resolve_round_and_payouts ---
    pub fn resolve_round_and_payouts(ctx: Context<ResolveRoundAndPayouts>) -> Result<()> {
        let game_session = &mut ctx.accounts.game_session_account;
        verify_dealer_signer(game_session, &ctx.accounts.dealer_account)?; 
        if game_session.game_state != GameState::RoundOver { return err!(BlackjackError::NotRoundOverState); }

        let dealer_final_score = game_session.dealer_hand.calculate_score().0;
        let dealer_is_busted = game_session.dealer_hand.status == HandStatus::Busted;
        let dealer_has_blackjack = game_session.dealer_hand.status == HandStatus::Blackjack;

        msg!("Resolving round. Dealer score: {} (Busted: {}, Blackjack: {})", dealer_final_score, dealer_is_busted, dealer_has_blackjack);
        
        let mut remaining_accounts_iter = ctx.remaining_accounts.iter();
        let table_name_bytes = game_session.table_name.as_bytes();
        let game_session_bump = game_session.bump;
        let seeds_game_session_pda = &[NORMALIZED_TABLE_NAME_PREFIX, table_name_bytes, &[game_session_bump]][..];
        let signer_seeds_for_game_session_pda = &[&seeds_game_session_pda[..]];

        for seat_index in 0..game_session.player_seats.len() {
            // Клонируем неизменяемые данные из player_seat перед изменяемым заимствованием game_session
            let player_seat_clone = game_session.player_seats[seat_index].clone();

            if !player_seat_clone.is_active_in_round || player_seat_clone.player_pubkey.is_none() { continue; }
            
            let player_pubkey = player_seat_clone.player_pubkey.unwrap();
            let bet_token_mint = player_seat_clone.current_bet_token_mint.ok_or(ProgramError::SecurityViolation)?;

            msg!("Processing player: {:?} at seat {}", player_pubkey, seat_index);

            // Получаем аккаунты для текущего игрока из remaining_accounts
            let player_token_account_info = next_account_info(&mut remaining_accounts_iter).map_err(|_| BlackjackError::InvalidRemainingAccountsForPayout)?;
            let game_escrow_account_info = next_account_info(&mut remaining_accounts_iter).map_err(|_| BlackjackError::InvalidRemainingAccountsForPayout)?;

            // Проверки на эти аккаунты
            let player_token_account_loaded: Account<TokenAccount> = Account::try_from(player_token_account_info)?; // Загружаем для проверки
            if player_token_account_loaded.owner != player_pubkey { return err!(BlackjackError::PlayerPayoutAccountOwnerMismatch); }
            if player_token_account_loaded.mint != bet_token_mint { return err!(BlackjackError::PlayerPayoutAccountMintMismatch); }
            
            // Проверка эскроу-счета (минт и что это PDA) - сложнее без bump эскроу здесь.
            // Предполагаем, что клиент передал правильный эскроу.
            let game_escrow_account_loaded: Account<TokenAccount> = Account::try_from(game_escrow_account_info)?;
            if game_escrow_account_loaded.mint != bet_token_mint {return err!(BlackjackError::BetTokenMintMismatch);}
            // Authority должен быть game_session PDA
            // let expected_escrow_authority = Pubkey::create_program_address(&[NORMALIZED_TABLE_NAME_PREFIX, table_name_bytes, &[game_session_bump]], ctx.program_id)?;
            // if game_escrow_account_loaded.authority != expected_escrow_authority { return err!(BlackjackError::EscrowAuthorityMismatch); }

            for hand_index in 0..player_seat_clone.hands.len() {
                let hand = &player_seat_clone.hands[hand_index]; // Используем клон
                let player_hand_score = hand.calculate_score().0;
                let player_is_busted = hand.status == HandStatus::Busted;
                let player_has_blackjack = hand.status == HandStatus::Blackjack;
                let effective_bet_amount = hand.get_effective_bet()?;

                msg!("  Hand {}: score {}, status {:?}, bet {}", hand_index, player_hand_score, hand.status, effective_bet_amount);

                let mut profit_for_player: u64 = 0; 
                let mut bet_returned_to_player: u64 = 0;
                let mut bet_to_dealer_profit: u64 = 0;

                if player_is_busted {
                    msg!("    Player Busted. Loses bet.");
                    bet_to_dealer_profit = effective_bet_amount;
                } else if player_has_blackjack {
                    if dealer_has_blackjack {
                        msg!("    Player Blackjack, Dealer Blackjack. Push.");
                        bet_returned_to_player = effective_bet_amount;
                    } else {
                        msg!("    Player Blackjack! Wins 1.3x profit.");
                        profit_for_player = (effective_bet_amount * BLACKJACK_PAYOUT_MULTIPLIER_NUMERATOR) / BLACKJACK_PAYOUT_MULTIPLIER_DENOMINATOR;
                        bet_returned_to_player = effective_bet_amount;
                    }
                } else if dealer_is_busted {
                    msg!("    Dealer Busted. Player wins 1:1 profit.");
                    profit_for_player = effective_bet_amount * WIN_PAYOUT_MULTIPLIER;
                    bet_returned_to_player = effective_bet_amount;
                } else if dealer_has_blackjack { 
                    msg!("    Dealer Blackjack. Player loses bet.");
                    bet_to_dealer_profit = effective_bet_amount;
                } else { 
                    if player_hand_score > dealer_final_score {
                        msg!("    Player score {} > Dealer score {}. Player wins 1:1 profit.", player_hand_score, dealer_final_score);
                        profit_for_player = effective_bet_amount * WIN_PAYOUT_MULTIPLIER;
                        bet_returned_to_player = effective_bet_amount;
                    } else if player_hand_score < dealer_final_score {
                        msg!("    Player score {} < Dealer score {}. Player loses bet.", player_hand_score, dealer_final_score);
                        bet_to_dealer_profit = effective_bet_amount;
                    } else { 
                        msg!("    Player score {} == Dealer score {}. Push.", player_hand_score, dealer_final_score);
                        bet_returned_to_player = effective_bet_amount;
                    }
                }
                
                game_session.add_dealer_profit(bet_token_mint, bet_to_dealer_profit)?;
                
                let total_transfer_to_player = bet_returned_to_player.checked_add(profit_for_player).ok_or(BlackjackError::PayoutError)?;
                if total_transfer_to_player > 0 {
                    msg!("    Transferring {} of token {} to player {}", total_transfer_to_player, bet_token_mint, player_pubkey);
                    token::transfer(
                        CpiContext::new_with_signer(
                            ctx.accounts.token_program.to_account_info(), 
                            Transfer {
                                from: game_escrow_account_info.clone(), 
                                to: player_token_account_info.clone(),   
                                authority: game_session.to_account_info(), 
                            },
                            signer_seeds_for_game_session_pda ), 
                        total_transfer_to_player
                    )?;
                }
            } 
        } 
        game_session.reset_hands_for_new_round();
        if !game_session.bets_acceptance_stopped {
            game_session.game_state = GameState::AcceptingBets;
            msg!("Round resolved. Game state back to AcceptingBets.");
        } else {
            msg!("Round resolved. Bets acceptance is stopped. Game state remains RoundOver.");
        }
        if game_session.current_deck_index >= DECK_RESHUFFLE_THRESHOLD_INDEX {
            msg!("Deck reshuffle recommended. Current index: {} (Threshold: {})",
                 game_session.current_deck_index, DECK_RESHUFFLE_THRESHOLD_INDEX);
        }
        Ok(())
    }
    
    // --- 3.12. dealer_reshuffle_deck ---
    pub fn dealer_reshuffle_deck(ctx: Context<DealerActionWithClock>, new_shuffle_seed_nonce: u64) -> Result<()> {
        let game_session = &mut ctx.accounts.game_session_account;
        verify_dealer_signer(game_session, &ctx.accounts.dealer_account)?;

        if game_session.game_state != GameState::AcceptingBets && game_session.game_state != GameState::RoundOver {
            return err!(BlackjackError::InvalidGameStateForDeal); 
        }
        let new_seed_hash = generate_shuffle_seed_hash( ctx.accounts.clock.slot, ctx.accounts.clock.unix_timestamp, &ctx.accounts.dealer_account.key(), new_shuffle_seed_nonce);
        game_session.shuffle_deck(new_seed_hash)?;
        msg!("Deck reshuffled by dealer. New seed hash: {:?}", new_seed_hash);
        Ok(())
    }

    // --- 3.13. dealer_toggle_bet_acceptance ---
    pub fn dealer_toggle_bet_acceptance(ctx: Context<DealerAction>, stop_bets: bool) -> Result<()> {
        let game_session = &mut ctx.accounts.game_session_account;
        verify_dealer_signer(game_session, &ctx.accounts.dealer_account)?;
        game_session.bets_acceptance_stopped = stop_bets;
        if stop_bets { msg!("Dealer has stopped bet acceptance for new rounds."); } 
        else {
            msg!("Dealer has enabled bet acceptance for new rounds.");
            if game_session.game_state == GameState::RoundOver {
                game_session.game_state = GameState::AcceptingBets;
                msg!("Game state changed to AcceptingBets.");
            }
        }
        Ok(())
    }

    // --- 3.14. dealer_withdraw_profit ---
    pub fn dealer_withdraw_profit(ctx: Context<DealerWithdrawProfit>, token_mint_to_withdraw: Pubkey) -> Result<()> {
        let game_session = &mut ctx.accounts.game_session_account;
        verify_dealer_signer(game_session, &ctx.accounts.dealer_account)?;
        
        let profit_entry_amount = game_session.dealer_profit_tracker.iter().find(|entry| entry.mint == token_mint_to_withdraw)
                                   .map(|e| e.amount).ok_or(BlackjackError::TokenMintNotInProfitTracker)?;
        if profit_entry_amount == 0 { return err!(BlackjackError::NoProfitToWithdraw); }

        if ctx.accounts.owner_fee_spl_token_account.mint != token_mint_to_withdraw { return err!(BlackjackError::OwnerFeeAccountMintMismatch); }
        if ctx.accounts.owner_fee_spl_token_account.owner != game_session.owner_fee_recipient { return err!(BlackjackError::OwnerFeeAccountOwnerMismatch); }
        if ctx.accounts.dealer_spl_token_account.mint != token_mint_to_withdraw { return err!(BlackjackError::DealerProfitAccountMintMismatch); }
        if ctx.accounts.dealer_spl_token_account.owner != game_session.dealer { return err!(BlackjackError::DealerProfitAccountOwnerMismatch); }
        if ctx.accounts.game_session_spl_escrow_account.mint != token_mint_to_withdraw { return err!(BlackjackError::BetTokenMintMismatch); }

        let fee_amount = profit_entry_amount.checked_mul(OWNER_FEE_BPS).ok_or(BlackjackError::ArithmeticOverflow)?
            .checked_div(BASIS_POINTS_DIVISOR).ok_or(BlackjackError::ArithmeticOverflow)?;
        let dealer_net_profit = profit_entry_amount.checked_sub(fee_amount).ok_or(BlackjackError::ArithmeticOverflow)?;

        msg!("Withdrawing profit for token {}: Total {}, Fee {}, Net {}", token_mint_to_withdraw, profit_entry_amount, fee_amount, dealer_net_profit);

        let table_name_bytes = game_session.table_name.as_bytes();
        let game_session_bump = game_session.bump;
        let seeds_gs_pda = &[NORMALIZED_TABLE_NAME_PREFIX, table_name_bytes, &[game_session_bump]][..];
        let signer_gs_pda = &[&seeds_gs_pda[..]];
        
        if fee_amount > 0 {
            token::transfer( CpiContext::new_with_signer( ctx.accounts.token_program.to_account_info(), Transfer {
                    from: ctx.accounts.game_session_spl_escrow_account.to_account_info(),
                    to: ctx.accounts.owner_fee_spl_token_account.to_account_info(),
                    authority: game_session.to_account_info(), // game_session PDA
                }, signer_gs_pda), fee_amount)?;
        }
        if dealer_net_profit > 0 {
            token::transfer( CpiContext::new_with_signer( ctx.accounts.token_program.to_account_info(), Transfer {
                    from: ctx.accounts.game_session_spl_escrow_account.to_account_info(),
                    to: ctx.accounts.dealer_spl_token_account.to_account_info(),
                    authority: game_session.to_account_info(), // game_session PDA
                }, signer_gs_pda), dealer_net_profit)?;
        }
        game_session.reduce_dealer_profit(token_mint_to_withdraw, profit_entry_amount)?;
        game_session.dealer_profit_tracker.retain(|entry| !(entry.mint == token_mint_to_withdraw && entry.amount == 0) );
        Ok(())
    }

    // --- 3.15. dealer_close_table ---
    pub fn dealer_close_table(ctx: Context<DealerCloseTable>) -> Result<()> {
        let game_session = &ctx.accounts.game_session_account; // Не mut, т.к. закрываем
        verify_dealer_signer(game_session, &ctx.accounts.dealer_account_signer)?;

        if !(game_session.game_state == GameState::AcceptingBets || game_session.game_state == GameState::RoundOver) { return err!(BlackjackError::CannotCloseTableActiveGame); }
        if game_session.player_seats.iter().any(|s| s.player_pubkey.is_some() && s.is_active_in_round) { return err!(BlackjackError::CannotCloseTableActiveGame); }
        if game_session.dealer_profit_tracker.iter().any(|entry| entry.amount > 0) { return err!(BlackjackError::CannotCloseTableActiveGame); /* Требуем сначала вывести весь профит */ }
        
        let usdc_mint_pubkey = Pubkey::from_str(USDC_MINT_PUBKEY_STR).map_err(|_| BlackjackError::PubkeyParseError)?;
        if ctx.accounts.dealer_usdc_token_account.mint != usdc_mint_pubkey { return err!(BlackjackError::UsdcMintMismatch); }

        let amount_to_return = game_session.dealer_locked_usdc_amount;
        let table_name_bytes = game_session.table_name.as_bytes();
        let game_session_bump_val = game_session.bump;
        let seeds_for_game_session_pda = &[NORMALIZED_TABLE_NAME_PREFIX, table_name_bytes, &[game_session_bump_val]][..];
        let signer_seeds = &[&seeds_for_game_session_pda[..]];

        token::transfer( CpiContext::new_with_signer( ctx.accounts.token_program.to_account_info(), Transfer {
                from: ctx.accounts.dealer_usdc_escrow_account.to_account_info(),
                to: ctx.accounts.dealer_usdc_token_account.to_account_info(),
                authority: game_session.to_account_info(), // game_session PDA как authority эскроу
            }, signer_seeds), amount_to_return)?;
        msg!("Returned {} USDC collateral to dealer", amount_to_return);

        token::close_account( CpiContext::new_with_signer( ctx.accounts.token_program.to_account_info(), CloseAccount {
                account: ctx.accounts.dealer_usdc_escrow_account.to_account_info(),
                destination: ctx.accounts.dealer_account_signer.to_account_info(), 
                authority: game_session.to_account_info(),    // game_session PDA как authority эскроу
            }, signer_seeds))?;
        msg!("Dealer USDC escrow account closed.");
        
        msg!("Game session account for table '{}' closed by dealer.", game_session.table_name);
        Ok(())
    }
} 

// --- Определения структур Accounts для каждой инструкции ---

#[derive(Accounts)]
#[instruction(table_name_input: String)] 
pub struct InitializeTable<'info> {
    #[account(
        init,
        payer = dealer,
        space = GameSession::CALCULATED_LEN, // 8 для дискриминатора уже включены Anchor
        seeds = [NORMALIZED_TABLE_NAME_PREFIX, normalize_and_validate_table_name(&table_name_input).unwrap().as_bytes()],
        bump
    )]
    pub game_session_account: Account<'info, GameSession>,

    #[account(mut)]
    pub dealer: Signer<'info>, 

    #[account(
        init,
        payer = dealer,
        seeds = [DEALER_USDC_ESCROW_SEED, game_session_account.key().as_ref()], 
        bump,
        token::mint = usdc_mint,
        token::authority = game_session_account, 
    )]
    pub dealer_usdc_escrow_account: Account<'info, TokenAccount>,

    #[account( mut, constraint = dealer_usdc_token_account.owner == dealer.key() @ BlackjackError::PlayerNotSigner, // Используем общую ошибку, можно уточнить
               constraint = dealer_usdc_token_account.mint == usdc_mint.key() @ BlackjackError::UsdcMintMismatch )]
    pub dealer_usdc_token_account: Account<'info, TokenAccount>, 

    pub usdc_mint: Account<'info, Mint>, 

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>, // rent нужен для init
    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
pub struct JoinTable<'info> {
    #[account(mut)] 
    pub game_session_account: Account<'info, GameSession>,
    #[account(mut)] 
    pub player_account: Signer<'info>, 
}

#[derive(Accounts)]
pub struct LeaveTable<'info> {
    #[account(mut)]
    pub game_session_account: Account<'info, GameSession>,
    #[account(mut)] // player_account может не быть mut, если он только Signer
    pub player_account: Signer<'info>,
}

#[derive(Accounts)]
#[instruction(seat_index: u8, amount_staked_ui: u64, usd_value_of_bet: u64)] // Аргументы для PDA эскроу
pub struct PlaceBet<'info> {
    #[account(mut)]
    pub game_session_account: Account<'info, GameSession>,
    
    #[account(mut)]
    pub player_account: Signer<'info>,

    #[account( mut, constraint = player_spl_token_account.owner == player_account.key() @ BlackjackError::PlayerNotSigner )]
    pub player_spl_token_account: Account<'info, TokenAccount>,

    #[account(
        init_if_needed, 
        payer = player_account, 
        seeds = [ BET_ESCROW_SEED, game_session_account.key().as_ref(), player_spl_token_account.mint.as_ref() ],
        bump,
        token::mint = player_spl_token_account.mint, 
        token::authority = game_session_account, 
    )]
    pub game_session_spl_escrow_account: Account<'info, TokenAccount>, 

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>, 
    pub rent: Sysvar<'info, Rent>,             
}

#[derive(Accounts)]
pub struct DealInitialCards<'info> {
    #[account(mut)]
    pub game_session_account: Account<'info, GameSession>,
    #[account( constraint = game_session_account.dealer == dealer_account.key() @ BlackjackError::DealerNotSigner )]
    pub dealer_account: Signer<'info>, 
}

#[derive(Accounts)]
pub struct PlayerAction<'info> { 
    #[account(mut)]
    pub game_session_account: Account<'info, GameSession>,
    pub player_account: Signer<'info>, // Игрок, который совершает действие
}

#[derive(Accounts)]
#[instruction(seat_index: u8, hand_index: u8)] // Аргументы для определения эскроу в constraint, если нужно
pub struct PlayerActionDoubleOrSplit<'info> { 
    #[account(mut)]
    pub game_session_account: Account<'info, GameSession>,
    
    #[account(mut)]
    pub player_account: Signer<'info>,

    #[account( mut, constraint = player_spl_token_account.owner == player_account.key() @ BlackjackError::PlayerNotSigner )]
    pub player_spl_token_account: Account<'info, TokenAccount>, 

    #[account( mut, seeds = [ BET_ESCROW_SEED, game_session_account.key().as_ref(), player_spl_token_account.mint.as_ref() ], bump,
               constraint = game_session_spl_escrow_account.mint == player_spl_token_account.mint @ BlackjackError::BetTokenMintMismatch )]
    pub game_session_spl_escrow_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}


#[derive(Accounts)]
pub struct DealerAction<'info> { 
    #[account(mut)]
    pub game_session_account: Account<'info, GameSession>,
    #[account( constraint = game_session_account.dealer == dealer_account.key() @ BlackjackError::DealerNotSigner )]
    pub dealer_account: Signer<'info>,
}

#[derive(Accounts)]
pub struct DealerActionWithClock<'info> { 
    #[account(mut)]
    pub game_session_account: Account<'info, GameSession>,
    #[account( constraint = game_session_account.dealer == dealer_account.key() @ BlackjackError::DealerNotSigner )]
    pub dealer_account: Signer<'info>,
    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
pub struct ResolveRoundAndPayouts<'info> {
    #[account(mut)]
    pub game_session_account: Account<'info, GameSession>,
    #[account( constraint = game_session_account.dealer == dealer_account.key() @ BlackjackError::DealerNotSigner )]
    pub dealer_account: Signer<'info>, 
    pub token_program: Program<'info, Token>,
    // remaining_accounts:
    // Для каждого активного игрока: player_spl_token_account (writable), game_session_spl_escrow_account (writable)
}

#[derive(Accounts)]
#[instruction(token_mint_to_withdraw: Pubkey)] // Для PDA эскроу
pub struct DealerWithdrawProfit<'info> {
    #[account(mut)]
    pub game_session_account: Account<'info, GameSession>,

    #[account( constraint = game_session_account.dealer == dealer_account.key() @ BlackjackError::DealerNotSigner )]
    pub dealer_account: Signer<'info>,

    #[account( mut, constraint = owner_fee_spl_token_account.owner == game_session_account.owner_fee_recipient @ BlackjackError::OwnerFeeAccountOwnerMismatch,
                    constraint = owner_fee_spl_token_account.mint == token_mint_to_withdraw @ BlackjackError::OwnerFeeAccountMintMismatch )]
    pub owner_fee_spl_token_account: Account<'info, TokenAccount>,

    #[account( mut, constraint = dealer_spl_token_account.owner == dealer_account.key() @ BlackjackError::DealerProfitAccountOwnerMismatch,
                    constraint = dealer_spl_token_account.mint == token_mint_to_withdraw @ BlackjackError::DealerProfitAccountMintMismatch )]
    pub dealer_spl_token_account: Account<'info, TokenAccount>, 

    #[account( mut, seeds = [ BET_ESCROW_SEED, game_session_account.key().as_ref(), token_mint_to_withdraw.as_ref() ], bump,
               constraint = game_session_spl_escrow_account.mint == token_mint_to_withdraw @ BlackjackError::BetTokenMintMismatch )]
    pub game_session_spl_escrow_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct DealerCloseTable<'info> {
    #[account( mut, close = dealer_account_signer )]
    pub game_session_account: Account<'info, GameSession>,

    #[account(mut)]
    pub dealer_account_signer: Signer<'info>, 

    #[account( mut, constraint = dealer_usdc_token_account.owner == dealer_account_signer.key() @ BlackjackError::PlayerNotSigner, // или спец. ошибка
                    constraint = dealer_usdc_token_account.mint.to_string() == USDC_MINT_PUBKEY_STR @ BlackjackError::UsdcMintMismatch )] // Сравнение строк не идеально, лучше Pubkey
    pub dealer_usdc_token_account: Account<'info, TokenAccount>,

    #[account( mut, seeds = [DEALER_USDC_ESCROW_SEED, game_session_account.key().as_ref()], bump = game_session_account.dealer_usdc_escrow_bump,
               constraint = dealer_usdc_escrow_account.mint.to_string() == USDC_MINT_PUBKEY_STR @ BlackjackError::UsdcMintMismatch )]
    pub dealer_usdc_escrow_account: Account<'info, TokenAccount>, 

    pub token_program: Program<'info, Token>,
}