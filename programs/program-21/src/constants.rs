use anchor_lang::prelude::*;
use anchor_lang::solana_program::pubkey;

// Длина имени стола
pub const TABLE_NAME_MIN_LEN: usize = 3;
pub const TABLE_NAME_MAX_LEN: usize = 16;
pub const NORMALIZED_TABLE_NAME_PREFIX: &[u8] = b"twentyone_table";

// --- Pyth Network ---
// Официальный ID программы Pyth Receiver в сети Solana.
pub const PYTH_RECEIVER_PROGRAM_ID: Pubkey = pubkey!("gSbePebfvPy7tRqimPoVecS2FqfpnffSGDMiu4c9pch");

// Параметры игры
pub const MAX_PLAYERS_LIMIT: u8 = 6;
pub const MIN_PLAYERS_FOR_DEAL: u8 = 1;
pub const NUM_DECKS: u8 = 4;
pub const CARDS_IN_DECK: u16 = 52;
pub const TOTAL_CARDS: u16 = NUM_DECKS as u16 * CARDS_IN_DECK;
pub const DECK_RESHUFFLE_THRESHOLD_INDEX: u16 = (TOTAL_CARDS * 3) / 4; // 75%

// Минимальное количество карт, необходимое для начала нового раунда.
pub const MAX_CARDS_IN_HAND: usize = 11; // Примерное макс. кол-во карт в руке

// Время на ход игрока
pub const PLAYER_TURN_TIMEOUT_SECONDS: i64 = 15;

// Выплаты
/// Числитель для расчета ПРОФИТА от блэкджека. Выплата 1.3x (13/10).
pub const BLACKJACK_PAYOUT_PROFIT_NUMERATOR: u64 = 13; 
/// Знаменатель для расчета ПРОФИТА от блэкджека.
pub const BLACKJACK_PAYOUT_PROFIT_DENOMINATOR: u64 = 10;
pub const TWENTYONE_PAYOUT_MULTIPLIER_DENOMINATOR: u64 = 10;
pub const WIN_PAYOUT_MULTIPLIER: u64 = 1; // Чистая выплата 1x от ставки

// Комиссия платформы
pub const OWNER_FEE_BPS: u64 = 2200; // 22% (1000 basis points = 10%)
pub const BASIS_POINTS_DIVISOR: u64 = 10000;

// Сиды для PDA
pub const BET_ESCROW_SEED: &[u8] = b"bet_escrow";


pub const DEFAULT_OWNER_FEE_RECIPIENT_STR: &str = "DDx7B6zkNhseqcp8Ym5JnP6YyRtMJ19cAML7EtfNz3CX";

pub const USDC_MINT_PUBKEY: Pubkey = pubkey!("DejYKjJTMYx6zWLAHdukSFbRyuLjiBFSQx68s7MZADJU");

pub const MAX_HANDS_PER_PLAYER: usize = 2; // Максимум 1 сплит

// Pubkey администратора, который имеет право изменять конфигурацию авторизации
pub const CONFIG_UPDATE_AUTHORITY_STR: &str = "GazSGmVPxgrwzhX4RUQGPddRfmSkXca3dmuHAgmYiJdd";

// Допустимое проскальзывание при проверке цен оракула (в базисных пунктах. 10 = 0.1%)
pub const PAYOUT_PRICE_SLIPPAGE_BPS: u64 = 300; // 3% slippage tolerance

pub const MAX_DIFFERENT_TOKENS_IN_PROFIT: usize = 50;

