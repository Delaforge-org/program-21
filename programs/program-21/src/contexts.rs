use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount, Mint};
use crate::state::{GameSession, TableAuthorityConfig};
use crate::constants::*;
use crate::errors::TwentyOneError;
use crate::utils::normalize_and_validate_table_name;

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
        seeds = [BET_ESCROW_SEED, game_session_account.key().as_ref(), usdc_mint.key().as_ref()], 
        bump,
        token::mint = usdc_mint,
        token::authority = game_session_account, 
    )]
    pub usdc_escrow_account: Account<'info, TokenAccount>,
    
    #[account( mut, constraint = dealer_usdc_token_account.owner == dealer.key() @ TwentyOneError::PlayerNotSigner,
               constraint = dealer_usdc_token_account.mint == usdc_mint.key() @ TwentyOneError::UsdcMintMismatch )]
    pub dealer_usdc_token_account: Account<'info, TokenAccount>, 

    pub usdc_mint: Account<'info, Mint>, 

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
pub struct JoinTable<'info> {
    #[account(mut)] 
    pub game_session_account: Account<'info, GameSession>,
    pub player_to_seat: SystemAccount<'info>,

    #[account(
        constraint = backend_signer.key() == authority_config.backend_authority @ TwentyOneError::UnauthorizedBackendSigner
    )]
    pub backend_signer: Signer<'info>,

    #[account(
        seeds = [TableAuthorityConfig::SEED_PREFIX],
        bump = authority_config.bump
    )]
    pub authority_config: Account<'info, TableAuthorityConfig>,
}

#[derive(Accounts)]
pub struct LeaveTable<'info> {
    #[account(mut)]
    pub game_session_account: Account<'info, GameSession>,
    #[account(mut)]
    pub player_account: Signer<'info>,
}

#[derive(Accounts)]
#[instruction(seat_index: u8, amount_staked_ui: u64, usd_value_of_bet: u64)]
pub struct PlaceBet<'info> {
    #[account(mut)]
    pub game_session_account: Account<'info, GameSession>,
    
    #[account(mut)]
    pub player_account: Signer<'info>,

    #[account(
        constraint = backend_signer.key() == authority_config.backend_authority @ TwentyOneError::UnauthorizedBackendSigner
    )]
    pub backend_signer: Signer<'info>,

    #[account(
        seeds = [TableAuthorityConfig::SEED_PREFIX],
        bump = authority_config.bump
    )]
    pub authority_config: Account<'info, TableAuthorityConfig>,

    #[account( mut, constraint = player_spl_token_account.owner == player_account.key() @ TwentyOneError::PlayerNotSigner )]
    pub player_spl_token_account: Account<'info, TokenAccount>,

    #[account(constraint = spl_token_mint.key() == player_spl_token_account.mint @ TwentyOneError::BetTokenMintMismatch)]
    pub spl_token_mint: Account<'info, Mint>,

    #[account(
        init_if_needed, 
        payer = player_account, 
        seeds = [ BET_ESCROW_SEED, game_session_account.key().as_ref(), spl_token_mint.key().as_ref() ],
        bump,
        token::mint = spl_token_mint, 
        token::authority = game_session_account, 
    )]
    pub game_session_spl_escrow_account: Account<'info, TokenAccount>, 

    /// CHECK: This is the Pyth price feed account for the token being bet
    pub pyth_price_feed: AccountInfo<'info>,
    pub clock: Sysvar<'info, Clock>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>, 
    pub rent: Sysvar<'info, Rent>,             
}

#[derive(Accounts)]
pub struct PlayerAction<'info> { 
    #[account(mut)]
    pub game_session_account: Account<'info, GameSession>,
    pub player_account: Signer<'info>,
    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
#[instruction(seat_index: u8, hand_index: u8)]
pub struct PlayerActionDoubleOrSplit<'info> { 
    #[account(mut)]
    pub game_session_account: Account<'info, GameSession>,
    
    #[account(mut)]
    pub player_account: Signer<'info>,

    #[account( mut, constraint = player_spl_token_account.owner == player_account.key() @ TwentyOneError::PlayerNotSigner )]
    pub player_spl_token_account: Account<'info, TokenAccount>, 

    #[account( mut, seeds = [ BET_ESCROW_SEED, game_session_account.key().as_ref(), player_spl_token_account.mint.as_ref() ], bump,
               constraint = game_session_spl_escrow_account.mint == player_spl_token_account.mint @ TwentyOneError::BetTokenMintMismatch )]
    pub game_session_spl_escrow_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
pub struct DealerAction<'info> { 
    #[account(mut)]
    pub game_session_account: Account<'info, GameSession>,
    #[account(constraint = game_session_account.dealer == dealer.key() @ TwentyOneError::DealerNotSigner)]
    pub dealer: Signer<'info>,
}

#[derive(Accounts)]
pub struct ResolveRoundAndPayouts<'info> {
    #[account(mut)]
    pub game_session_account: Account<'info, GameSession>,

    #[account(
        mut, 
        constraint = backend_signer.key() == authority_config.backend_authority @ TwentyOneError::UnauthorizedBackendSigner
    )]
    pub backend_signer: Signer<'info>, 
    
    #[account(
        seeds = [TableAuthorityConfig::SEED_PREFIX],
        bump = authority_config.bump
    )]
    pub authority_config: Account<'info, TableAuthorityConfig>,

    pub token_program: Program<'info, Token>,
    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
#[instruction(amount_to_withdraw_ui: u64, token_mint_to_withdraw: Pubkey)]
pub struct DealerWithdrawProfit<'info> {
    #[account(mut)]
    pub game_session_account: Account<'info, GameSession>,

    #[account( constraint = game_session_account.dealer == dealer_account.key() @ TwentyOneError::DealerNotSigner )]
    pub dealer_account: Signer<'info>,

    #[account( 
        mut, 
        constraint = owner_fee_spl_token_account.mint == token_mint_to_withdraw @ TwentyOneError::OwnerFeeAccountMintMismatch,
        constraint = owner_fee_spl_token_account.owner.to_string() == DEFAULT_OWNER_FEE_RECIPIENT_STR @ TwentyOneError::OwnerFeeAccountOwnerMismatch
    )]
    pub owner_fee_spl_token_account: Account<'info, TokenAccount>,

    #[account( mut, constraint = dealer_spl_token_account.owner == dealer_account.key() @ TwentyOneError::DealerProfitAccountOwnerMismatch,
                    constraint = dealer_spl_token_account.mint == token_mint_to_withdraw @ TwentyOneError::DealerProfitAccountMintMismatch )]
    pub dealer_spl_token_account: Account<'info, TokenAccount>, 

    #[account( mut, seeds = [ BET_ESCROW_SEED, game_session_account.key().as_ref(), token_mint_to_withdraw.as_ref() ], bump,
               constraint = game_session_spl_escrow_account.mint == token_mint_to_withdraw @ TwentyOneError::BetTokenMintMismatch )]
    pub game_session_spl_escrow_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
pub struct DealerCloseTable<'info> {
    #[account(mut, close = dealer, has_one = dealer @ TwentyOneError::DealerNotSigner)]
    pub game_session_account: Account<'info, GameSession>,
    
    /// CHECK: Получатель ренты при закрытии game_session
    #[account(mut)]
    pub dealer: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct ForcePlayerAction<'info> {
    #[account(mut)]
    pub game_session_account: Account<'info, GameSession>,
    #[account(mut)]
    pub fee_payer: Signer<'info>,
    pub clock: Sysvar<'info, Clock>,
}

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

    #[account(
        mut,
        constraint = admin.key().to_string() == CONFIG_UPDATE_AUTHORITY_STR @ TwentyOneError::Unauthorized
    )]
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

    #[account(
        mut,
        constraint = admin.key().to_string() == CONFIG_UPDATE_AUTHORITY_STR @ TwentyOneError::Unauthorized
    )]
    pub admin: Signer<'info>,
}

#[derive(Accounts)]
pub struct BackendAuthorizedAction<'info> {
    #[account(mut)]
    pub game_session_account: Account<'info, GameSession>,

    #[account(
        mut,
        constraint = backend_signer.key() == authority_config.backend_authority @ TwentyOneError::UnauthorizedBackendSigner
    )]
    pub backend_signer: Signer<'info>,

    #[account(
        seeds = [TableAuthorityConfig::SEED_PREFIX],
        bump = authority_config.bump
    )]
    pub authority_config: Account<'info, TableAuthorityConfig>,
    pub clock: Sysvar<'info, Clock>,
}

/// Контекст для новой единой функции финализации раунда.
/// Объединяет в себе проверку и выплаты.
#[derive(Accounts)]
pub struct FinalizeRound<'info> {
    #[account(mut)]
    pub game_session_account: Account<'info, GameSession>,

    #[account(
        // Проверяем, что вызов авторизован бэкендом
        constraint = backend_signer.key() == authority_config.backend_authority @ TwentyOneError::UnauthorizedBackendSigner
    )]
    pub backend_signer: Signer<'info>,

    #[account(seeds = [TableAuthorityConfig::SEED_PREFIX], bump = authority_config.bump)]
    pub authority_config: Account<'info, TableAuthorityConfig>,

    pub token_program: Program<'info, Token>,
    pub clock: Sysvar<'info, Clock>,
}
