#![allow(deprecated)]

use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount, Transfer};
use anchor_spl::token::spl_token;
use pyth_sdk::PriceFeed;

// Локальные модули
mod constants;
mod errors;
mod state;
mod utils;
mod contexts;

// Импорт из локальных модулей для удобства
use constants::*;
use errors::TwentyOneError; // Прямой импорт перечисления
use state::*;
use utils::*;
use contexts::*;


// --- События (Events) ---
// ПЕРЕМЕЩЕНЫ НАВЕРХ ДЛЯ ГАРАНТИРОВАННОЙ ОБЛАСТИ ВИДИМОСТИ

#[event]
pub struct TableCreated {
    pub table_name: String,
    pub dealer: Pubkey,
}

#[event]
pub struct TableClosed {
    pub table_name: String,
    pub dealer: Pubkey,
}

#[event]
pub struct PlayerJoined {
    pub table_name: String,
    pub player: Pubkey,
    pub seat_index: u8,
}

#[event]
pub struct PlayerLeft {
    pub table_name: String,
    pub player: Pubkey,
    pub seat_index: u8,
}

#[event]
pub struct BetPlaced {
    pub table_name: String,
    pub player: Pubkey,
    pub seat_index: u8,
    pub amount: u64,
    pub token_mint: Pubkey,
}

#[event]
pub struct RoundStarted {
    pub table_name: String,
    pub dealer_up_card: Card,
    pub player_hands: Vec<InitialPlayerHand>,
}

#[event]
pub struct PlayerActed {
    pub table_name: String,
    pub player: Pubkey,
    pub seat_index: u8,
    pub hand_index: u8,
    pub action: PlayerActionType,
    pub new_card: Option<Card>,
}

#[event]
pub struct RoundFinished {
    pub table_name: String,
    pub dealer_hand: Vec<Card>,
    pub dealer_score: u8,
    pub results: Vec<PlayerHandResult>,
}

#[event]
pub struct TableClosingDown {
    pub table_name: String,
}

#[event]
pub struct DeckShuffled {
    pub table_name: String,
}

// --- ОСНОВНАЯ ЛОГИКА ПРОГРАММЫ ---

declare_id!("9KXtH1oFkFU3wey5BDxQbjLepFFvGQCQMtjs6zsfs3Dr"); // ЗАГЛУШКА ID, замените!

// Вспомогательная функция для обработки одного escrow аккаунта (ВЫНЕСЕНА ЗА ПРЕДЕЛЫ #[program])
fn process_escrow_account<'info>(
    escrow_account_info: &AccountInfo<'info>,
    dealer_token_account_info: &AccountInfo<'info>,
    game_session: &Account<'info, GameSession>,
    dealer: &UncheckedAccount<'info>,
    token_program: &Program<'info, Token>,
    authority_seeds: &[&[u8]],
) -> Result<()> {
    // Проверяем баланс escrow аккаунта
    let escrow_data = escrow_account_info.try_borrow_data()?;
    let escrow_account = TokenAccount::try_deserialize(&mut &escrow_data[..])?;
    let amount = escrow_account.amount;
    drop(escrow_data);

    // Если есть токены, переводим их
    if amount > 0 {
        let transfer_instruction = spl_token::instruction::transfer(
            &spl_token::id(),
            escrow_account_info.key,
            dealer_token_account_info.key,
            &game_session.key(),
            &[],
            amount,
        )?;

        anchor_lang::solana_program::program::invoke_signed(
            &transfer_instruction,
            &[
                escrow_account_info.clone(),
                dealer_token_account_info.clone(),
                game_session.to_account_info(),
                token_program.to_account_info(),
            ],
            &[authority_seeds],
        )?;
    }

    // Закрываем escrow аккаунт
    let close_instruction = spl_token::instruction::close_account(
        &spl_token::id(),
        escrow_account_info.key,
        &dealer.key(),
        &game_session.key(),
        &[],
    )?;

    anchor_lang::solana_program::program::invoke_signed(
        &close_instruction,
        &[
            escrow_account_info.clone(),
            dealer.to_account_info(),
            game_session.to_account_info(),
            token_program.to_account_info(),
        ],
        &[authority_seeds],
    )?;

    Ok(())
}

// Вспомогательная функция для переводов токенов (ВЫНЕСЕНА ЗА ПРЕДЕЛЫ #[program])
fn execute_token_transfers(
    ctx: &Context<ExecutePayouts>,
    payouts: &[PayoutInstruction],
    game_session: &Account<GameSession>,
) -> Result<()> {
    let table_name_bytes = game_session.table_name.as_bytes();
    let bump_seed = [game_session.bump];
    let signer_seeds = &[&[NORMALIZED_TABLE_NAME_PREFIX, table_name_bytes, &bump_seed][..]];
    
    for payout in payouts.iter() {
        if payout.amount > 0 {
            let cpi_accounts = Transfer {
                from: ctx.remaining_accounts[payout.escrow_account_index as usize].to_account_info(),
                to: ctx.remaining_accounts[payout.player_account_index as usize].to_account_info(),
                authority: game_session.to_account_info(),
            };
            
            let cpi_program = ctx.accounts.token_program.to_account_info();
            let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds);
            
            anchor_spl::token::transfer(cpi_ctx, payout.amount)?;
        }
    }
    Ok(())
}

#[program]
pub mod program_21 {
    use super::*; // Для доступа к items из constants, errors, state, utils, и событий

    // --- NEW: initialize_authority_config ---
    pub fn initialize_authority_config(ctx: Context<InitializeAuthorityConfig>, backend_authority_pubkey: Pubkey) -> Result<()> {
        let authority_config = &mut ctx.accounts.authority_config;
        authority_config.backend_authority = backend_authority_pubkey;
        authority_config.bump = ctx.bumps.authority_config;
        Ok(())
    }

    // --- NEW: update_authority_config ---
    pub fn update_authority_config(ctx: Context<UpdateAuthorityConfig>, new_backend_authority: Pubkey) -> Result<()> {
        ctx.accounts.authority_config.backend_authority = new_backend_authority;
        Ok(())
    }

    // --- 3.1. initialize_table ---
    pub fn initialize_table(
        ctx: Context<InitializeTable>,
        table_name_input: String,
        dealer_collateral_usd: u64,
        shuffle_seed_nonce: u64,
    ) -> Result<()> {
        let game_session = &mut ctx.accounts.game_session_account;
        let dealer = &ctx.accounts.dealer;
        let clock = &ctx.accounts.clock;

        let normalized_table_name = normalize_and_validate_table_name(&table_name_input)?;
        
        if dealer_collateral_usd == 0 { return err!(TwentyOneError::MinBetIsZero); }
        
        if ctx.accounts.dealer_usdc_token_account.mint != ctx.accounts.usdc_mint.key() { return err!(TwentyOneError::UsdcMintMismatch); }

        anchor_spl::token::transfer( CpiContext::new( ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.dealer_usdc_token_account.to_account_info(),
                    to: ctx.accounts.usdc_escrow_account.to_account_info(),
                    authority: dealer.to_account_info(),
                }),
            dealer_collateral_usd
        )?;

        game_session.table_name = normalized_table_name;
        game_session.dealer = dealer.key();
        game_session.dealer_locked_usdc_amount = dealer_collateral_usd;
        game_session.game_state = GameState::AcceptingBets;
        
        let seed_hash = generate_shuffle_seed_hash( clock.slot, clock.unix_timestamp, &dealer.key(), shuffle_seed_nonce);
        game_session.shuffle_deck(seed_hash)?;
        
        game_session.dealer_hand = Hand::default();
        game_session.player_seats = vec![PlayerSeat::default(); MAX_PLAYERS_LIMIT as usize];
        game_session.bump = ctx.bumps.game_session_account;

        emit!(TableCreated {
            table_name: game_session.table_name.clone(),
            dealer: game_session.dealer,
        });

        Ok(())
    }

    // --- 3.2. join_table (ЗАЩИЩЕНАЯ ВЕРСИЯ) ---
    pub fn join_table(ctx: Context<JoinTable>, seat_index: u8) -> Result<()> {
        let game_session = &mut ctx.accounts.game_session_account;
        let player_to_seat = &ctx.accounts.player_to_seat; 
        let seat_idx = seat_index as usize;

        if seat_idx >= game_session.player_seats.len() { return err!(TwentyOneError::InvalidSeatIndex); }
        if game_session.player_seats[seat_idx].player_pubkey.is_some() { return err!(TwentyOneError::SeatTaken); }

        game_session.player_seats[seat_idx].player_pubkey = Some(player_to_seat.key());
        game_session.player_seats[seat_idx].reset_for_new_round(); 

        emit!(PlayerJoined {
            table_name: game_session.table_name.clone(),
            player: player_to_seat.key(),
            seat_index,
        });

        Ok(())
    }

    // --- 3.3. leave_table ---
    pub fn leave_table(ctx: Context<LeaveTable>, seat_index: u8) -> Result<()> {
        let game_session = &mut ctx.accounts.game_session_account;
        let player_account = &ctx.accounts.player_account;
        let seat_idx = seat_index as usize;

        verify_player_at_seat(game_session, player_account.as_ref(), seat_index)?;
        if game_session.player_seats[seat_idx].is_active_in_round { return err!(TwentyOneError::PlayerHasActiveBet); }
        if (game_session.game_state != GameState::AcceptingBets && game_session.game_state != GameState::RoundOver) &&
           !game_session.player_seats[seat_idx].hands.is_empty() {
            return err!(TwentyOneError::PlayerHasActiveBet);
        }
        
        game_session.player_seats[seat_idx].player_pubkey = None;
        game_session.player_seats[seat_idx].reset_for_new_round();

        emit!(PlayerLeft {
            table_name: game_session.table_name.clone(),
            player: player_account.key(),
            seat_index,
        });

        Ok(())
    }

    // --- 3.4. place_bet (С ПРОВЕРКОЙ ЦЕНЫ) ---
    pub fn place_bet(
        ctx: Context<PlaceBet>,
        seat_index: u8,
        amount_staked_ui: u64, 
        usd_value_of_bet: u64,
    ) -> Result<()> {
        let game_session = &mut ctx.accounts.game_session_account;
        let player_account = &ctx.accounts.player_account;
        let token_mint_key = ctx.accounts.player_spl_token_account.mint;

        if game_session.game_state != GameState::AcceptingBets { return err!(TwentyOneError::NotAcceptingBets); }

        verify_player_at_seat(game_session, player_account.as_ref(), seat_index)?;
        let seat_idx = seat_index as usize;

        if game_session.player_seats[seat_idx].is_active_in_round { return err!(TwentyOneError::PlayerHasActiveBet); }
        
        let price_feed_account = &ctx.accounts.pyth_price_feed;
        if *price_feed_account.owner != PYTH_RECEIVER_PROGRAM_ID { return err!(TwentyOneError::InvalidPriceFeedOwner); }

        let price_feed = PriceFeed::try_from_slice(&price_feed_account.data.borrow())
            .map_err(|_| error!(TwentyOneError::PriceFeedStale))?;
        let price = price_feed.get_price_unchecked();

        let calculated_value_usd = (amount_staked_ui as u128)
            .checked_mul(price.price as u128).ok_or(TwentyOneError::ArithmeticOverflow)?
            .checked_div(10u128.pow(price.expo.abs() as u32)).ok_or(TwentyOneError::ArithmeticOverflow)?;

        let slippage_amount = (usd_value_of_bet as u128 * PAYOUT_PRICE_SLIPPAGE_BPS as u128) / 10000;
        let lower_bound = (usd_value_of_bet as u128).saturating_sub(slippage_amount);
        let upper_bound = (usd_value_of_bet as u128).saturating_add(slippage_amount);
        
        if calculated_value_usd < lower_bound || calculated_value_usd > upper_bound {
             return err!(TwentyOneError::PayoutCalculationMismatch);
        }

        anchor_spl::token::transfer( CpiContext::new( ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.player_spl_token_account.to_account_info(),
                    to: ctx.accounts.game_session_spl_escrow_account.to_account_info(),
                    authority: ctx.accounts.player_account.to_account_info(),
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

        emit!(BetPlaced {
            table_name: game_session.table_name.clone(),
            player: player_account.key(),
            seat_index,
            amount: amount_staked_ui,
            token_mint: token_mint_key,
        });

        Ok(())
    }

    // --- 3.5. deal_initial_cards (ЗАЩИЩЕНАЯ ВЕРСИЯ) ---
    pub fn deal_initial_cards(ctx: Context<BackendAuthorizedAction>) -> Result<()> {
        let game_session = &mut ctx.accounts.game_session_account;
        
        if game_session.game_state != GameState::AcceptingBets { return err!(TwentyOneError::InvalidGameStateForDeal); }

        if game_session.current_deck_index >= DECK_RESHUFFLE_THRESHOLD_INDEX {
            let seed_hash = generate_shuffle_seed_hash(
                ctx.accounts.clock.slot,
                ctx.accounts.clock.unix_timestamp,
                &ctx.accounts.backend_signer.key(),
                game_session.current_deck_index as u64,
            );
            game_session.shuffle_deck(seed_hash)?;
            emit!(DeckShuffled {
                table_name: game_session.table_name.clone(),
            });
        }

        let active_player_count = game_session.player_seats.iter().filter(|s| s.is_active_in_round).count();
        if active_player_count < MIN_PLAYERS_FOR_DEAL as usize { return err!(TwentyOneError::NotEnoughPlayers); }
        
        game_session.dealer_hand = Hand::default();

        for seat_idx in 0..game_session.player_seats.len() {
            if game_session.player_seats[seat_idx].is_active_in_round {
                let card = game_session.draw_card()?;
                if let Some(hand) = game_session.player_seats[seat_idx].hands.get_mut(0) {
                    hand.add_card(card);
                }
            }
        }
        let card_for_dealer_1 = game_session.draw_card()?;
        game_session.dealer_hand.add_card(card_for_dealer_1);

        for seat_idx in 0..game_session.player_seats.len() {
            if game_session.player_seats[seat_idx].is_active_in_round {
                let card = game_session.draw_card()?;
                if let Some(hand) = game_session.player_seats[seat_idx].hands.get_mut(0) {
                    hand.add_card(card);
                }
            }
        }
        let card_for_dealer_2 = game_session.draw_card()?;
        game_session.dealer_hand.add_card(card_for_dealer_2);
        
        let mut all_players_have_blackjack_or_resolved = true;
        let mut initial_hands_for_event: Vec<InitialPlayerHand> = Vec::with_capacity(active_player_count);

        for seat_idx in 0..game_session.player_seats.len() {
            if game_session.player_seats[seat_idx].is_active_in_round {
                if let Some(hand) = game_session.player_seats[seat_idx].hands.get_mut(0) {
                    if hand.is_blackjack() { hand.status = HandStatus::Blackjack; } 
                    else { all_players_have_blackjack_or_resolved = false; }
                }
                let player_seat = &game_session.player_seats[seat_idx];
                initial_hands_for_event.push(InitialPlayerHand {
                    player: player_seat.player_pubkey.ok_or(ProgramError::InvalidInstructionData)?,
                    seat_index: seat_idx as u8,
                    hand: player_seat.hands.get(0).ok_or(ProgramError::InvalidInstructionData)?.cards.clone(),
                });
            }
        }

        if all_players_have_blackjack_or_resolved && active_player_count > 0 {
            game_session.game_state = GameState::RoundOver;
        } else {
            game_session.game_state = GameState::PlayerTurns;
            let first_player_to_act_idx = game_session.player_seats.iter().position(
                |s| s.is_active_in_round && s.hands.get(0).map_or(false, |h| h.status != HandStatus::Blackjack)
            ); 
            if let Some(idx) = first_player_to_act_idx {
                game_session.current_turn_seat_index = Some(idx as u8);
                game_session.current_turn_hand_index = Some(0);
                game_session.current_turn_start_timestamp = Some(ctx.accounts.clock.unix_timestamp);
            } else {
                game_session.game_state = GameState::RoundOver;
            }
        }
        
        emit!(RoundStarted {
            table_name: game_session.table_name.clone(),
            dealer_up_card: *game_session.dealer_hand.cards.get(0).ok_or(ProgramError::InvalidInstructionData)?,
            player_hands: initial_hands_for_event,
        });

        Ok(())
    }
    
    // --- 3.6. player_action_hit ---
    pub fn player_action_hit(ctx: Context<PlayerAction>, seat_index: u8, hand_index: u8) -> Result<()> {
        let game_session = &mut ctx.accounts.game_session_account;
        verify_player_turn_and_hand(game_session, ctx.accounts.player_account.as_ref(), seat_index, hand_index)?;
        
        let table_name = game_session.table_name.clone();
        let player_key = ctx.accounts.player_account.key();
        
        let new_card = game_session.draw_card()?;
        let hand = game_session.player_seats[seat_index as usize].hands.get_mut(hand_index as usize)
            .ok_or_else(|| error!(TwentyOneError::InvalidHandIndex))?;

        hand.add_card(new_card);
        hand.update_status_after_card_drawn();
        
        emit!(PlayerActed {
            table_name,
            player: player_key,
            seat_index,
            hand_index,
            action: PlayerActionType::Hit,
            new_card: Some(new_card),
        });
        
        if hand.status != HandStatus::Playing {
            determine_next_player_or_transition_to_dealer(game_session, ctx.accounts.clock.unix_timestamp)?;
        }
        Ok(())
    }

    // --- 3.7. player_action_stand ---
    pub fn player_action_stand(ctx: Context<PlayerAction>, seat_index: u8, hand_index: u8) -> Result<()> {
        let game_session = &mut ctx.accounts.game_session_account;
        verify_player_turn_and_hand(game_session, ctx.accounts.player_account.as_ref(), seat_index, hand_index)?;

        let hand = game_session.player_seats[seat_index as usize].hands.get_mut(hand_index as usize)
            .ok_or_else(|| error!(TwentyOneError::InvalidHandIndex))?;
        
        hand.status = HandStatus::Stood;

        emit!(PlayerActed {
            table_name: game_session.table_name.clone(),
            player: ctx.accounts.player_account.key(),
            seat_index,
            hand_index,
            action: PlayerActionType::Stand,
            new_card: None,
        });

        determine_next_player_or_transition_to_dealer(game_session, ctx.accounts.clock.unix_timestamp)?;
        Ok(())
    }

    // --- 3.8. player_action_double_down ---
    pub fn player_action_double_down(ctx: Context<PlayerActionDoubleOrSplit>, seat_index: u8, hand_index: u8) -> Result<()> {
        let game_session = &mut ctx.accounts.game_session_account;
        verify_player_turn_and_hand(game_session, ctx.accounts.player_account.as_ref(), seat_index, hand_index)?;
        
        let hand_token_mint_for_bet;
        let hand_original_bet_amount;
        
        {
            let player_seat_check = &game_session.player_seats[seat_index as usize];
            let hand_check = player_seat_check.hands.get(hand_index as usize).ok_or(TwentyOneError::InvalidHandIndex)?;
            if hand_check.cards.len() != 2 { return err!(TwentyOneError::CannotDoubleNotTwoCards); }
            hand_token_mint_for_bet = hand_check.token_mint_for_bet;
            hand_original_bet_amount = hand_check.original_bet_amount;
        }

        if ctx.accounts.player_spl_token_account.mint != hand_token_mint_for_bet { return err!(TwentyOneError::BetTokenMintMismatch); }
        
        let additional_stake = hand_original_bet_amount;
        if ctx.accounts.player_spl_token_account.amount < additional_stake { return err!(TwentyOneError::InsufficientFundsForDoubleDown); }

        anchor_spl::token::transfer( CpiContext::new( ctx.accounts.token_program.to_account_info(), Transfer {
                from: ctx.accounts.player_spl_token_account.to_account_info(),
                to: ctx.accounts.game_session_spl_escrow_account.to_account_info(),
                authority: ctx.accounts.player_account.to_account_info(),
            }), additional_stake)?;

        let new_card = game_session.draw_card()?;
        let hand = game_session.player_seats[seat_index as usize].hands.get_mut(hand_index as usize).ok_or(TwentyOneError::InvalidHandIndex)?;
        hand.bet_multiplier_x100 = 200;
        hand.add_card(new_card);
        
        hand.update_status_after_card_drawn();
        if hand.status != HandStatus::Busted { hand.status = HandStatus::DoubledAndStood; }
        
        emit!(PlayerActed {
            table_name: game_session.table_name.clone(),
            player: ctx.accounts.player_account.key(),
            seat_index,
            hand_index,
            action: PlayerActionType::DoubleDown,
            new_card: Some(new_card),
        });

        determine_next_player_or_transition_to_dealer(game_session, ctx.accounts.clock.unix_timestamp)?;
        Ok(())
    }
    
    // --- 3.9. player_action_split ---
    pub fn player_action_split(ctx: Context<PlayerActionDoubleOrSplit>, seat_index: u8, hand_index: u8) -> Result<()> {
        if hand_index != 0 { return err!(TwentyOneError::CannotSplitAlreadySplit); }
        
        let game_session = &mut ctx.accounts.game_session_account;
        verify_player_turn_and_hand(game_session, ctx.accounts.player_account.as_ref(), seat_index, hand_index)?;

        let (original_hand_token_mint, original_hand_bet_amount);
        {
            let player_seat_check = &game_session.player_seats[seat_index as usize];
            if player_seat_check.hands.len() != 1 { return err!(TwentyOneError::CannotSplitAlreadySplit); }
            let original_hand_check = player_seat_check.hands.get(0).ok_or(TwentyOneError::InvalidHandIndex)?;
            if original_hand_check.cards.len() != 2 { return err!(TwentyOneError::CannotSplitNotTwoCards); }
            if original_hand_check.cards[0].default_value() != original_hand_check.cards[1].default_value() { return err!(TwentyOneError::CannotSplitRanksMismatch); }
            original_hand_token_mint = original_hand_check.token_mint_for_bet;
            original_hand_bet_amount = original_hand_check.original_bet_amount;
        }

        if ctx.accounts.player_spl_token_account.mint != original_hand_token_mint { return err!(TwentyOneError::BetTokenMintMismatch); }
        
        let stake_for_new_hand = original_hand_bet_amount;
        if ctx.accounts.player_spl_token_account.amount < stake_for_new_hand { return err!(TwentyOneError::InsufficientFundsForSplit); }

        anchor_spl::token::transfer( CpiContext::new( ctx.accounts.token_program.to_account_info(), Transfer {
                from: ctx.accounts.player_spl_token_account.to_account_info(),
                to: ctx.accounts.game_session_spl_escrow_account.to_account_info(),
                authority: ctx.accounts.player_account.to_account_info(),
            }), stake_for_new_hand)?;

        let table_name_for_event = game_session.table_name.clone();
        let player_key_for_event = ctx.accounts.player_account.key();

        let card_for_new_hand = {
            let player_seat = &mut game_session.player_seats[seat_index as usize];
            player_seat.hands[0].cards.pop().ok_or(ProgramError::InvalidInstructionData)?
        };
        
        let mut new_hand = Hand::new(original_hand_token_mint, stake_for_new_hand);
        new_hand.add_card(card_for_new_hand);

        let player_seat = &mut game_session.player_seats[seat_index as usize];
        player_seat.hands.push(new_hand);

        emit!(PlayerActed {
            table_name: table_name_for_event,
            player: player_key_for_event,
            seat_index,
            hand_index,
            action: PlayerActionType::Split,
            new_card: None,
        });

        for hand_idx_to_deal in 0..2 {
             let card_dealt = game_session.draw_card()?;
             let hand = game_session.player_seats[seat_index as usize].hands.get_mut(hand_idx_to_deal).unwrap();
             hand.add_card(card_dealt);

             if hand.cards[0].is_ace() { hand.status = HandStatus::Stood; } 
             else { hand.update_status_after_card_drawn(); }
        }
        
        let player_seat = &mut game_session.player_seats[seat_index as usize];
        if player_seat.hands[0].status != HandStatus::Playing {
            determine_next_player_or_transition_to_dealer(game_session, ctx.accounts.clock.unix_timestamp)?;
        } else {
            game_session.current_turn_start_timestamp = Some(ctx.accounts.clock.unix_timestamp);
        }
        Ok(())
    }

    // --- 3.10. dealer_play_turn (ЗАЩИЩЕНАЯ ВЕРСИЯ) ---
    pub fn dealer_play_turn(ctx: Context<BackendAuthorizedAction>) -> Result<()> {
        let game_session = &mut ctx.accounts.game_session_account;
        if game_session.game_state != GameState::DealerTurn { return err!(TwentyOneError::NotDealerTurnState); }

        loop {
            let (score, is_soft) = game_session.dealer_hand.calculate_score();
            
            if score > 21 {
                game_session.dealer_hand.status = HandStatus::Busted;
                break;
            }
            if score > 17 {
                game_session.dealer_hand.status = HandStatus::Stood;
                break;
            }
            if score == 17 {
                if is_soft {
                    let card = game_session.draw_card()?;
                    game_session.dealer_hand.add_card(card);
                } else {
                    game_session.dealer_hand.status = HandStatus::Stood;
                    break;
                }
            } else {
                let card = game_session.draw_card()?;
                game_session.dealer_hand.add_card(card);
            }
        }
        
        game_session.game_state = GameState::RoundOver;
        Ok(())
    }
    
    // --- NEW: dealer_prepare_to_close ---
    pub fn dealer_prepare_to_close(ctx: Context<DealerAction>) -> Result<()> {
        let game_session = &mut ctx.accounts.game_session_account;
        verify_dealer_signer(game_session, &ctx.accounts.dealer)?;
        
        game_session.closing_down = true;

        emit!(TableClosingDown {
            table_name: game_session.table_name.clone(),
        });

        Ok(())
    }
    
    // --- 3.11. resolve_round (ТОЛЬКО СВЕРКА РЕЗУЛЬТАТОВ) ---
    pub fn resolve_round(
        ctx: Context<ResolveRound>,
        backend_results: Vec<PlayerHandResult>, // Результаты от бэкенда
    ) -> Result<()> {
        let game_session = &mut ctx.accounts.game_session_account;
        
        if game_session.game_state != GameState::RoundOver { 
            return err!(TwentyOneError::NotRoundOverState); 
        }
        
        // Получаем данные дилера для сверки
        let dealer_final_score = game_session.dealer_hand.calculate_score().0;
        let dealer_is_busted = game_session.dealer_hand.status == HandStatus::Busted;
        let dealer_has_blackjack = game_session.dealer_hand.is_blackjack();
        
        // СВЕРЯЕМ результаты от бэкенда с реальным состоянием игры
        for backend_result in backend_results.iter() {
            let seat_idx = backend_result.seat_index as usize;
            let hand_idx = backend_result.hand_index as usize;
            
            // Проверяем что игрок существует
            let player_seat = game_session.player_seats.get(seat_idx)
                .ok_or(TwentyOneError::InvalidSeatIndex)?;
            
            if player_seat.player_pubkey.is_none() || !player_seat.is_active_in_round {
                return err!(TwentyOneError::SeatNotTaken);
            }
            
            if player_seat.player_pubkey.unwrap() != backend_result.player {
                return err!(TwentyOneError::PlayerMismatch);
            }
            
            // Проверяем что рука существует
            let hand = player_seat.hands.get(hand_idx)
                .ok_or(TwentyOneError::InvalidHandIndex)?;
            
            if hand.cards.is_empty() {
                return err!(TwentyOneError::HandNotFound);
            }
            
            // СВЕРЯЕМ карты руки
            if hand.cards != backend_result.hand_cards {
                return err!(TwentyOneError::HandCardsMismatch);
            }
            
            // СВЕРЯЕМ счет руки
            if hand.calculate_score().0 as u8 != backend_result.hand_score {
                return err!(TwentyOneError::HandScoreMismatch);
            }
            
            // СВЕРЯЕМ результат игры (самая важная проверка)
            let bet_usd_value: u128 = player_seat.current_bet_usd_value as u128;
            let effective_bet_usd = (bet_usd_value * hand.bet_multiplier_x100 as u128) / 100;
            
            let (expected_payout_usd, expected_outcome) = calculate_expected_usd_return(
                hand, effective_bet_usd, dealer_final_score, dealer_is_busted, dealer_has_blackjack
            )?;
            
            // Проверяем что бэкенд правильно рассчитал исход
            if backend_result.outcome != expected_outcome {
                return err!(TwentyOneError::OutcomeMismatch);
            }
            
            // Проверяем что бэкенд правильно рассчитал выплату (с учетом slippage)
            let expected_payout = expected_payout_usd as u64;
            let backend_payout = backend_result.payout;
            
            let slippage_amount = (expected_payout as u128 * PAYOUT_PRICE_SLIPPAGE_BPS as u128) / 10000;
            let lower_bound = (expected_payout as u128).saturating_sub(slippage_amount) as u64;
            let upper_bound = (expected_payout as u128).saturating_add(slippage_amount) as u64;
            
            if backend_payout < lower_bound || backend_payout > upper_bound {
                return err!(TwentyOneError::PayoutCalculationMismatch);
            }
        }
        
        // Если все проверки прошли - сбрасываем руки и меняем состояние
        game_session.reset_hands_for_new_round();
        if !game_session.closing_down {
            game_session.game_state = GameState::AcceptingBets;
        }
        
        // Отправляем событие с ПРОВЕРЕННЫМИ результатами от бэкенда
        emit!(RoundFinished {
            table_name: game_session.table_name.clone(),
            dealer_hand: game_session.dealer_hand.cards.clone(),
            dealer_score: dealer_final_score,
            results: backend_results, // Используем проверенные результаты бэкенда
        });

        Ok(())
    }

    // --- 3.12. execute_payouts (КЛОНИРОВАНИЕ ДАННЫХ) ---
    pub fn execute_payouts(
        ctx: Context<ExecutePayouts>,
        payouts: Vec<PayoutInstruction>,
        price_validations: Vec<PriceValidation>,
    ) -> Result<()> {
        let game_session = &ctx.accounts.game_session_account;
        
        // 1. КЛОНИРУЕМ все данные аккаунтов заранее
        let mut pyth_account_data = Vec::new();
        for validation in price_validations.iter() {
            let pyth_account = &ctx.remaining_accounts[validation.pyth_feed_index as usize];
            
            // Проверяем owner и клонируем данные
            if *pyth_account.owner != PYTH_RECEIVER_PROGRAM_ID {
                return err!(TwentyOneError::InvalidPriceFeedOwner);
            }
            
            let data_clone = pyth_account.data.borrow().clone();
            pyth_account_data.push(data_clone);
        }
        
        // 2. Работаем с клонированными данными Pyth (БЕЗ заимствований remaining_accounts)
        for (i, validation) in price_validations.iter().enumerate() {
            let price_feed = PriceFeed::try_from_slice(&pyth_account_data[i])
                .map_err(|_| error!(TwentyOneError::PriceFeedStale))?;
            let pyth_price = price_feed.get_price_unchecked();
            
            let price_diff = (pyth_price.price - validation.expected_price).abs();
            let max_diff = (validation.expected_price * PAYOUT_PRICE_SLIPPAGE_BPS as i64) / 10000;
            
            if price_diff > max_diff {
                return err!(TwentyOneError::PayoutCalculationMismatch);
            }
        }
        
        // 3. ПОСЛЕ освобождения всех заимствований Pyth - делаем переводы
        let table_name_bytes = game_session.table_name.as_bytes();
        let bump_seed = [game_session.bump];
        let signer_seeds = &[&[NORMALIZED_TABLE_NAME_PREFIX, table_name_bytes, &bump_seed][..]];
        
        for payout in payouts.iter() {
            if payout.amount > 0 {
                let cpi_accounts = Transfer {
                    from: ctx.remaining_accounts[payout.escrow_account_index as usize].to_account_info(),
                    to: ctx.remaining_accounts[payout.player_account_index as usize].to_account_info(),
                    authority: game_session.to_account_info(),
                };
                
                let cpi_program = ctx.accounts.token_program.to_account_info();
                let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds);
                
                anchor_spl::token::transfer(cpi_ctx, payout.amount)?;
            }
        }

        Ok(())
    }

    // --- 3.13. dealer_withdraw_profit (ПРОСТАЯ ПРОВЕРКА ОСТАТКОВ) ---
    pub fn dealer_withdraw_profit(
        ctx: Context<DealerWithdrawProfit>, 
        amount_to_withdraw_ui: u64,
        token_mint_to_withdraw: Pubkey,
        remaining_balances: Vec<TokenBalance>,  // Готовые цены от бэкенда
    ) -> Result<()> {
        let game_session = &ctx.accounts.game_session_account;
        
        // БЕЗ HashMap! Проверяем цены через прямое сравнение с Pyth
        let mut total_remaining_value_usd: u128 = 0;
        
        for (_balance_idx, balance) in remaining_balances.iter().enumerate() {
            // Бэкенд присылает индекс соответствующего Pyth price feed
            let pyth_feed_index = balance.pyth_feed_index as usize;
            let pyth_account = &ctx.remaining_accounts[pyth_feed_index];
            
            // Проверяем что это действительно Pyth price feed
            if *pyth_account.owner != PYTH_RECEIVER_PROGRAM_ID {
                return err!(TwentyOneError::InvalidPriceFeedOwner);
            }
            
            let price_feed = PriceFeed::try_from_slice(&pyth_account.data.borrow())
                .map_err(|_| error!(TwentyOneError::PriceFeedStale))?;
            let pyth_price = price_feed.get_price_unchecked();
            
            // Рассчитываем цену из Pyth
            let pyth_value_usd = (balance.amount as u128)
                .checked_mul(pyth_price.price as u128).ok_or(TwentyOneError::ArithmeticOverflow)?
                .checked_div(10u128.pow(pyth_price.expo.abs() as u32)).ok_or(TwentyOneError::ArithmeticOverflow)?;
            
            // Сверяем с готовой ценой от бэкенда (в рамках slippage)
            let backend_value_usd = balance.value_usd as u128;
            let slippage_amount = (backend_value_usd * PAYOUT_PRICE_SLIPPAGE_BPS as u128) / 10000;
            let lower_bound = backend_value_usd.saturating_sub(slippage_amount);
            let upper_bound = backend_value_usd.saturating_add(slippage_amount);
            
            if pyth_value_usd < lower_bound || pyth_value_usd > upper_bound {
                return err!(TwentyOneError::PayoutCalculationMismatch);
            }
            
            // Если проверка прошла - используем готовую цену от бэкенда
            total_remaining_value_usd = total_remaining_value_usd.checked_add(backend_value_usd)
                .ok_or(TwentyOneError::ArithmeticOverflow)?;
        }
        
        // ГЛАВНАЯ ПРОВЕРКА: остатки >= залога
        if total_remaining_value_usd < game_session.dealer_locked_usdc_amount as u128 {
            return err!(TwentyOneError::InsufficientBankValue);
        }
        
        // Если проверка прошла - выполняем вывод
        let fee_amount = amount_to_withdraw_ui.checked_mul(OWNER_FEE_BPS).ok_or(TwentyOneError::ArithmeticOverflow)?
            .checked_div(BASIS_POINTS_DIVISOR).ok_or(TwentyOneError::ArithmeticOverflow)?;
        let dealer_net_profit = amount_to_withdraw_ui.checked_sub(fee_amount).ok_or(TwentyOneError::ArithmeticOverflow)?;

        let escrow_seeds = &[
            BET_ESCROW_SEED,
            game_session.to_account_info().key.as_ref(),
            token_mint_to_withdraw.as_ref(),
            &[ctx.bumps.game_session_spl_escrow_account]
        ];
        let signer_seeds = &[&escrow_seeds[..]];

        if fee_amount > 0 {
            anchor_spl::token::transfer( CpiContext::new_with_signer( ctx.accounts.token_program.to_account_info(), Transfer {
                    from: ctx.accounts.game_session_spl_escrow_account.to_account_info(),
                    to: ctx.accounts.owner_fee_spl_token_account.to_account_info(),
                    authority: game_session.to_account_info(),
                }, signer_seeds), fee_amount )?;
        }

        if dealer_net_profit > 0 {
            anchor_spl::token::transfer( CpiContext::new_with_signer( ctx.accounts.token_program.to_account_info(), Transfer {
                    from: ctx.accounts.game_session_spl_escrow_account.to_account_info(),
                    to: ctx.accounts.dealer_spl_token_account.to_account_info(),
                    authority: game_session.to_account_info(),
                }, signer_seeds), dealer_net_profit)?;
        }
        
        Ok(())
    }

    // --- 3.15. dealer_close_table (УПРОЩЕННАЯ ВЕРСИЯ) ---
    pub fn dealer_close_table(ctx: Context<DealerCloseTable>) -> Result<()> {
        let game_session = &ctx.accounts.game_session_account;

        // Базовые проверки состояния
        if !(game_session.game_state == GameState::AcceptingBets || game_session.game_state == GameState::RoundOver) {
            return err!(TwentyOneError::CannotCloseTableActiveGame);
        }
        
        // Проверяем, что нет активных игроков
        if game_session.player_seats.iter().any(|s| s.player_pubkey.is_some() && s.is_active_in_round) {
            return err!(TwentyOneError::CannotCloseTableActiveGame);
        }

        // Проверяем, что escrow-счета пусты (опционально, для безопасности)
        // Если фронтенд гарантирует это, то можно убрать эту проверку
        for account_pair in ctx.remaining_accounts.chunks_exact(2) {
            let escrow_account_info = &account_pair[0];
            let escrow_data = escrow_account_info.try_borrow_data()?;
            let escrow_account = TokenAccount::try_deserialize(&mut &escrow_data[..])?;
            if escrow_account.amount > 0 {
                return err!(TwentyOneError::TableHasActiveEscrow); // Добавьте эту ошибку в errors.rs
            }
            drop(escrow_data);
        }

        emit!(TableClosed { 
            table_name: game_session.table_name.clone(), 
            dealer: game_session.dealer 
        });

        // Аккаунт game_session будет автоматически закрыт благодаря #[account(close = dealer)] в contexts.rs
        // Рента автоматически вернется дилеру
        
        Ok(())
    }

    // --- 3.16. force_player_action ---
    pub fn force_player_action(ctx: Context<ForcePlayerAction>, seat_index: u8, hand_index: u8, action: ForcedAction) -> Result<()> {
        let game_session = &mut ctx.accounts.game_session_account;
        let clock = &ctx.accounts.clock;

        if game_session.game_state != GameState::PlayerTurns { return err!(TwentyOneError::NotPlayerTurnsState); }
        
        let (current_seat, current_hand) = (game_session.current_turn_seat_index, game_session.current_turn_hand_index);
        if current_seat != Some(seat_index) || current_hand != Some(hand_index) { return err!(TwentyOneError::NotThisPlayerTurn); }
        
        let start_time = game_session.current_turn_start_timestamp.ok_or(TwentyOneError::TurnTimerNotSet)?;
        if clock.unix_timestamp <= start_time.checked_add(PLAYER_TURN_TIMEOUT_SECONDS).ok_or(TwentyOneError::ArithmeticOverflow)? {
             return err!(TwentyOneError::TurnTimeNotExpired);
        }

        let player_pubkey = game_session.player_seats[seat_index as usize].player_pubkey.ok_or(TwentyOneError::SeatNotTaken)?;
        let table_name_for_event = game_session.table_name.clone();
        
        match action {
            ForcedAction::Hit => {
                let new_card = game_session.draw_card()?;
                let hand = game_session.player_seats[seat_index as usize].hands.get_mut(hand_index as usize)
                    .ok_or(TwentyOneError::InvalidHandIndex)?;

                if hand.status != HandStatus::Playing { return err!(TwentyOneError::HandActionOnFinalizedHand); }
                
                hand.add_card(new_card);
                hand.update_status_after_card_drawn();

                emit!(PlayerActed {
                    table_name: table_name_for_event,
                    player: player_pubkey,
                    seat_index,
                    hand_index,
                    action: PlayerActionType::Hit,
                    new_card: Some(new_card),
                });

                if hand.status != HandStatus::Playing {
                    determine_next_player_or_transition_to_dealer(game_session, clock.unix_timestamp)?;
                }
            },
            ForcedAction::Stand => {
                let hand = game_session.player_seats[seat_index as usize].hands.get_mut(hand_index as usize)
                    .ok_or(TwentyOneError::InvalidHandIndex)?;
                if hand.status != HandStatus::Playing { return err!(TwentyOneError::HandActionOnFinalizedHand); }
                
                hand.status = HandStatus::Stood;

                emit!(PlayerActed {
                    table_name: table_name_for_event,
                    player: player_pubkey,
                    seat_index,
                    hand_index,
                    action: PlayerActionType::Stand,
                    new_card: None,
                });
                
                determine_next_player_or_transition_to_dealer(game_session, clock.unix_timestamp)?;
            },
            ForcedAction::Split => {
                let hand = game_session.player_seats[seat_index as usize].hands.get_mut(hand_index as usize)
                    .ok_or(TwentyOneError::InvalidHandIndex)?;
                if hand.status != HandStatus::Playing { return err!(TwentyOneError::HandActionOnFinalizedHand); }
                hand.status = HandStatus::Stood;
                 emit!(PlayerActed {
                    table_name: table_name_for_event,
                    player: player_pubkey,
                    seat_index,
                    hand_index,
                    action: PlayerActionType::Stand,
                    new_card: None,
                });
                determine_next_player_or_transition_to_dealer(game_session, clock.unix_timestamp)?;
            }
        }
        
        Ok(())
    }
} 