use anchor_lang::prelude::*;

// Длина имени стола
pub const TABLE_NAME_MIN_LEN: usize = 3;
pub const TABLE_NAME_MAX_LEN: usize = 32;
pub const NORMALIZED_TABLE_NAME_PREFIX: &[u8] = b"game_session";

// Параметры игры
pub const MAX_PLAYERS_LIMIT: u8 = 6;
pub const MIN_PLAYERS_FOR_DEAL: u8 = 1;
pub const NUM_DECKS: u8 = 4;
pub const CARDS_IN_DECK: u16 = 52;
pub const TOTAL_CARDS: u16 = NUM_DECKS as u16 * CARDS_IN_DECK;
// Порог для перетасовки колоды (например, 75% карт использовано)
pub const DECK_RESHUFFLE_THRESHOLD_INDEX: u16 = (TOTAL_CARDS * 3) / 4; // 75%
pub const MAX_CARDS_IN_HAND: usize = 11; // Примерное макс. кол-во карт в руке

// Выплаты
pub const BLACKJACK_PAYOUT_MULTIPLIER_NUMERATOR: u64 = 13; // Чистая выплата 1.3x от ставки
pub const BLACKJACK_PAYOUT_MULTIPLIER_DENOMINATOR: u64 = 10;
pub const WIN_PAYOUT_MULTIPLIER: u64 = 1; // Чистая выплата 1x от ставки

// Комиссия платформы
pub const OWNER_FEE_BPS: u64 = 1000; // 10% (1000 basis points = 10%)
pub const BASIS_POINTS_DIVISOR: u64 = 10000;

// Сиды для PDA
pub const DEALER_USDC_ESCROW_SEED: &[u8] = b"dealer_usdc_escrow";
pub const BET_ESCROW_SEED: &[u8] = b"bet_escrow";

// Pubkey владельца контракта для получения комиссии (ЗАМЕНИТЬ НА РЕАЛЬНЫЙ)
// Пример: "owner11111111111111111111111111111111111111"
pub const DEFAULT_OWNER_FEE_RECIPIENT_STR: &str = "SysvarRent111111111111111111111111111111111"; // ЗАГЛУШКА, замените!

// Pubkey USDC минта (ЗАМЕНИТЬ НА РЕАЛЬНЫЙ USDC MINT НА MAINNET/DEVNET)
// Mainnet-beta USDC: EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZkfgmedr
// Devnet USDC (пример): Gh9ZwEmdkyGPnL7xZ6RXsP2K2W2nTKJ1hZAWcMQA7R2Z
pub const USDC_MINT_PUBKEY_STR: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZkfgmedr"; // ЗАГЛУШКА, замените!