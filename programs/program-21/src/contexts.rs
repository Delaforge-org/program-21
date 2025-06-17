use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};
use crate::state::{TableAuthorityConfig, GameSession};
use crate::constants::{BET_ESCROW_SEED, NORMALIZED_TABLE_NAME_PREFIX, USDC_MINT_PUBKEY};
use crate::errors::TwentyOneError;
use crate::utils::normalize_and_validate_table_name;

// --- КОНТЕКСТЫ ДЛЯ УПРАВЛЕНИЯ АВТОРИЗАЦИЕЙ ---

#[derive(Accounts)]
pub struct InitializeAuthorityConfig<'info> {
    #[account(
        init,
        payer = admin,
        space = TableAuthorityConfig::CALCULATED_LEN,
        seeds = [TableAuthorityConfig::SEED_PREFIX],
        bump
    )]
    pub authority_config: Account<'info, TableAuthorityConfig>,
    #[account(mut)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateAuthorityConfig<'info> {
    #[account(
        mut,
        seeds = [TableAuthorityConfig::SEED_PREFIX],
        bump = authority_config.bump
    )]
    pub authority_config: Account<'info, TableAuthorityConfig>,
    pub admin: Signer<'info>,
}


// --- КОНТЕКСТЫ ДЛЯ УПРАВЛЕНИЯ СТОЛОМ ---
#[derive(Accounts)]
#[instruction(table_name_input: String)]
pub struct InitializeTable<'info> {
    #[account(
        init,
        payer = dealer,
        space = GameSession::CALCULATED_LEN,
        seeds = [NORMALIZED_TABLE_NAME_PREFIX, normalize_and_validate_table_name(&table_name_input).unwrap().as_bytes()],
        bump
    )]
    pub game_session_account: Account<'info, GameSession>,

    #[account(mut)]
    pub dealer: Signer<'info>,
    
    #[account(
        init,
        payer = dealer,
        token::mint = usdc_mint,
        token::authority = game_session_account,
        seeds = [
            BET_ESCROW_SEED, 
            game_session_account.key().as_ref(),
            usdc_mint.key().as_ref()
        ],
        bump
    )]
    pub usdc_escrow_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = dealer_usdc_token_account.mint == usdc_mint.key()
    )]
    pub dealer_usdc_token_account: Account<'info, TokenAccount>,

    #[account(address = USDC_MINT_PUBKEY)]
    pub usdc_mint: Account<'info, Mint>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
    pub clock: Sysvar<'info, Clock>,
}


#[derive(Accounts)]
pub struct DealerAction<'info> {
    #[account(mut)]
    pub game_session_account: Account<'info, GameSession>,
    pub dealer: Signer<'info>,
}

#[derive(Accounts)]
pub struct DealerCloseTable<'info> {
    #[account(
        mut,
        close = dealer,
        has_one = dealer,
    )]
    pub game_session_account: Account<'info, GameSession>,

    #[account(mut)]
    pub dealer: Signer<'info>,
}


// --- КОНТЕКСТЫ ДЛЯ ДЕЙСТВИЙ ИГРОКОВ ---

#[derive(Accounts)]
#[instruction(seat_index: u8)]
pub struct JoinTable<'info> {
    #[account(mut)]
    pub game_session_account: Account<'info, GameSession>,
    #[account(mut)]
    pub player_to_seat: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(seat_index: u8)]
pub struct LeaveTable<'info> {
    #[account(mut)]
    pub game_session_account: Account<'info, GameSession>,
    #[account(mut)]
    pub player_account: Signer<'info>,
}

#[derive(Accounts)]
pub struct PlaceBet<'info> {
    #[account(mut)]
    pub game_session_account: Account<'info, GameSession>,

    #[account(mut)]
    pub player_spl_token_account: Account<'info, TokenAccount>,
    
    pub spl_token_mint: Account<'info, Mint>,

    #[account(
        init_if_needed,
        payer = player_account,
        token::mint = spl_token_mint,
        token::authority = game_session_account,
        seeds = [
            BET_ESCROW_SEED, 
            game_session_account.key().as_ref(),
            spl_token_mint.key().as_ref()
        ],
        bump
    )]
    pub game_session_spl_escrow_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub player_account: Signer<'info>,

    /// CHECK: This is a Pyth price feed account. It is validated in the instruction logic.
    #[account(mut)]
    pub pyth_price_feed: AccountInfo<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct PlayerAction<'info> {
    #[account(mut)]
    pub game_session_account: Account<'info, GameSession>,
    
    #[account(mut)]
    pub player_account: Signer<'info>,
    
    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
pub struct PlayerActionDoubleOrSplit<'info> {
    #[account(mut)]
    pub game_session_account: Account<'info, GameSession>,

    #[account(mut)]
    pub player_account: Signer<'info>,

    #[account(
        mut,
        // The escrow for this specific token must already exist
        seeds = [
            BET_ESCROW_SEED, 
            game_session_account.key().as_ref(),
            player_spl_token_account.mint.as_ref()
        ],
        bump,
    )]
    pub game_session_spl_escrow_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub player_spl_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub clock: Sysvar<'info, Clock>,
}

// --- СПЕЦИАЛИЗИРОВАННЫЕ КОНТЕКСТЫ ---

#[derive(Accounts)]
pub struct BackendAuthorizedAction<'info> {
    #[account(mut)]
    pub game_session_account: Account<'info, GameSession>,

    #[account(
        mut,
        constraint = backend_signer.key() == authority_config.backend_authority @ TwentyOneError::BackendSignerMismatch
    )]
    pub backend_signer: Signer<'info>,
    
    #[account(
        seeds = [TableAuthorityConfig::SEED_PREFIX],
        bump
    )]
    pub authority_config: Account<'info, TableAuthorityConfig>,
    
    pub clock: Sysvar<'info, Clock>,
}


#[derive(Accounts)]
pub struct ForcePlayerAction<'info> {
    #[account(mut)]
    pub game_session_account: Account<'info, GameSession>,
    
    #[account(
        mut,
        constraint = caller.key() == authority_config.backend_authority || caller.key() == game_session_account.dealer
            @ TwentyOneError::UnauthorizedForceAction
    )]
    pub caller: Signer<'info>,
    
    #[account(
        seeds = [TableAuthorityConfig::SEED_PREFIX],
        bump
    )]
    pub authority_config: Account<'info, TableAuthorityConfig>,
    
    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
pub struct DealerWithdrawProfit<'info> {
    #[account(
        mut,
        has_one = dealer
    )]
    pub game_session_account: Account<'info, GameSession>,

    #[account(mut)]
    pub dealer: Signer<'info>,
    
    #[account(
        mut,
        seeds = [
            BET_ESCROW_SEED, 
            game_session_account.key().as_ref(),
            dealer_spl_token_account.mint.as_ref()
        ],
        bump
    )]
    pub game_session_spl_escrow_account: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub dealer_spl_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    /// CHECK: The recipient of the owner's fee.
    pub owner_fee_spl_token_account: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
}

// --- КОНТЕКСТ ДЛЯ ФИНАЛИЗАЦИИ РАУНДА ---

#[derive(Accounts)]
pub struct FinalizeRound<'info> {
    #[account(
        mut,
        seeds = [
            NORMALIZED_TABLE_NAME_PREFIX,
            game_session_account.table_name.as_bytes()
        ],
        bump = game_session_account.bump,
        constraint = backend_signer.key() == authority_config.backend_authority @ TwentyOneError::BackendSignerMismatch
    )]
    pub game_session_account: Account<'info, GameSession>,

    #[account(
        seeds = [TableAuthorityConfig::SEED_PREFIX],
        bump,
    )]
    pub authority_config: Account<'info, TableAuthorityConfig>,

    pub backend_signer: Signer<'info>,

    pub token_program: Program<'info, Token>,
}
