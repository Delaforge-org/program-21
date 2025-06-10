// programs/blackjack_anchor/src/utils.rs

use anchor_lang::prelude::*;
use crate::state::{Card, Suit, Rank, GameState, GameSession, HandStatus}; // Добавил GameState, GameSession, HandStatus
use crate::constants::*;
use crate::errors::BlackjackError;
use sha2::{Sha256, Digest};
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

// Нормализация и валидация имени стола
pub fn normalize_and_validate_table_name(name_input: &str) -> Result<String> {
    let normalized = name_input.to_lowercase();
    if normalized.len() < TABLE_NAME_MIN_LEN { return err!(BlackjackError::TableNameTooShort); }
    if normalized.len() > TABLE_NAME_MAX_LEN { return err!(BlackjackError::TableNameTooLong); }
    let mut prev_char_is_hyphen = false;
    for (i, c) in normalized.chars().enumerate() {
        if !((c >= 'a' && c <= 'z') || (c >= '0' && c <= '9') || c == '-') {
            return err!(BlackjackError::TableNameInvalidChars);
        }
        if c == '-' {
            if i == 0 || i == normalized.len() - 1 { return err!(BlackjackError::TableNameInvalidHyphenPlacement); }
            if prev_char_is_hyphen { return err!(BlackjackError::TableNameConsecutiveHyphens); }
            prev_char_is_hyphen = true;
        } else {
            prev_char_is_hyphen = false;
        }
    }
    Ok(normalized)
}

// Создание стандартной колоды
pub fn create_standard_shoe(num_decks: u8) -> Vec<Card> {
    let mut shoe = Vec::with_capacity((num_decks as u16 * CARDS_IN_DECK) as usize);
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
        return err!(BlackjackError::DealerNotSigner);
    }
    Ok(())
}

// Проверка, является ли игрок тем, кто сидит на указанном месте
pub fn verify_player_at_seat(
    game_session: &Account<GameSession>,
    player_signer: &Signer,
    seat_index: u8,
) -> Result<()> {
    let seat_idx = seat_index as usize;
    if seat_idx >= game_session.player_seats.len() {
        return err!(BlackjackError::InvalidSeatIndex);
    }
    match game_session.player_seats[seat_idx].player_pubkey {
        Some(key) if key == player_signer.key() => Ok(()),
        _ => err!(BlackjackError::PlayerNotAtSeatIndex), // Или более общая NotPlayerTurn
    }
}

// Проверка, является ли сейчас ход этого игрока на этом месте и для этой руки
pub fn verify_player_turn_and_hand(
    game_session: &Account<GameSession>,
    player_signer: &Signer,
    seat_index: u8,
    hand_index: u8,
) -> Result<()> {
    // 1. Проверка состояния игры
    if game_session.game_state != GameState::PlayerTurns {
        return err!(BlackjackError::NotPlayerTurnsState);
    }
    // 2. Проверка, что игрок действительно сидит на этом месте
    verify_player_at_seat(game_session, player_signer, seat_index)?;

    // 3. Проверка, что это его очередь по seat_index
    match game_session.current_turn_seat_index {
        Some(current_seat_idx) if current_seat_idx == seat_index => {},
        _ => return err!(BlackjackError::WrongSeatForTurn),
    }
    
    // 4. Проверка, что это его очередь по hand_index
    match game_session.current_turn_hand_index {
        Some(current_hand_idx) if current_hand_idx == hand_index => {},
        _ => return err!(BlackjackError::WrongHandForTurn),
    }
    
    // 5. Проверка, что рука, которой пытаются играть, действительно активна (Playing)
    let seat_idx_usize = seat_index as usize;
    let hand_idx_usize = hand_index as usize;
    if let Some(hand) = game_session.player_seats.get(seat_idx_usize)
                       .and_then(|seat| seat.hands.get(hand_idx_usize)) {
        if hand.status != HandStatus::Playing {
            return err!(BlackjackError::HandActionOnFinalizedHand);
        }
    } else {
        return err!(BlackjackError::InvalidHandIndex); // Рука не найдена
    }

    Ok(())
}

// Определение следующего игрока/руки или переход к дилеру
pub fn determine_next_player_or_transition_to_dealer(game_session: &mut Account<GameSession>) -> Result<()> {
    let mut found_next_turn = false;

    if let (Some(current_seat_idx_u8), Some(current_hand_idx_u8)) = 
        (game_session.current_turn_seat_index, game_session.current_turn_hand_index) {
        
        let current_seat_idx = current_seat_idx_u8 as usize;
        let current_hand_idx = current_hand_idx_u8 as usize;

        // Попробовать следующую руку текущего игрока
        if current_hand_idx + 1 < game_session.player_seats[current_seat_idx].hands.len() {
            if game_session.player_seats[current_seat_idx].hands[current_hand_idx + 1].status == HandStatus::Playing {
                game_session.current_turn_hand_index = Some((current_hand_idx + 1) as u8);
                msg!("Next turn: Player seat {}, hand {}", current_seat_idx_u8, current_hand_idx + 1);
                found_next_turn = true;
            }
        }

        // Если у текущего игрока больше нет рук для игры, ищем следующего игрока
        if !found_next_turn {
            for i in 1..=game_session.max_players as usize {
                let next_potential_seat_idx = (current_seat_idx + i) % (game_session.max_players as usize);
                
                if game_session.player_seats[next_potential_seat_idx].is_active_in_round &&
                   game_session.player_seats[next_potential_seat_idx].player_pubkey.is_some() {
                    
                    if let Some(next_hand_to_play_idx) = game_session.player_seats[next_potential_seat_idx].get_first_active_hand_index() {
                        game_session.current_turn_seat_index = Some(next_potential_seat_idx as u8);
                        game_session.current_turn_hand_index = Some(next_hand_to_play_idx as u8);
                        msg!("Next turn: Player seat {}, hand {}", next_potential_seat_idx, next_hand_to_play_idx);
                        found_next_turn = true;
                        break; 
                    }
                }
            }
        }
    } else { // Этого не должно быть, если current_turn_seat_index всегда установлен правильно во время PlayerTurns
        msg!("Warning: current_turn_seat_index or current_turn_hand_index was None during player turn determination.");
        // Попытка найти первого игрока с нуля
        for seat_idx in 0..game_session.max_players as usize {
             if game_session.player_seats[seat_idx].is_active_in_round &&
                game_session.player_seats[seat_idx].player_pubkey.is_some() {
                 if let Some(hand_idx) = game_session.player_seats[seat_idx].get_first_active_hand_index() {
                     game_session.current_turn_seat_index = Some(seat_idx as u8);
                     game_session.current_turn_hand_index = Some(hand_idx as u8);
                     msg!("Next turn (re-evaluated): Player seat {}, hand {}", seat_idx, hand_idx);
                     found_next_turn = true;
                     break;
                 }
             }
        }
    }

    if !found_next_turn {
        // Если не нашли следующего игрока/руку для хода
        msg!("All players have completed their turns or no active players left to play.");
        game_session.game_state = GameState::DealerTurn;
        game_session.current_turn_seat_index = None; 
        game_session.current_turn_hand_index = None;
        msg!("Transitioning to DealerTurn.");
    }
    Ok(())
}