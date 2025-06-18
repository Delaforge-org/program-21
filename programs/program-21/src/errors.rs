use anchor_lang::prelude::*;

#[error_code]
pub enum TwentyOneError {
    // --- Ошибки валидации ---
    #[msg("Table name must be between 3 and 16 characters long.")]
    TableNameLengthInvalid,
    #[msg("Table name contains invalid characters. Only lowercase letters, numbers, and single hyphens are allowed.")]
    TableNameInvalidChars,
    #[msg("Table name cannot start or end with a hyphen.")]
    TableNameInvalidHyphenPlacement,
    #[msg("Table name cannot have consecutive hyphens.")]
    TableNameConsecutiveHyphens,
    #[msg("The provided table name was not in its normalized (canonical) form. The client must normalize it before sending.")]
    TableNameNotNormalized,
    #[msg("The provided seat index is out of bounds for this table.")]
    InvalidSeatIndex,
    #[msg("The specified seat is already taken.")]
    SeatTaken,
    #[msg("The specified seat is not taken by any player.")]
    SeatNotTaken,
    #[msg("The signer is not the player at the specified seat index.")]
    PlayerNotAtSeatIndex,
    #[msg("The signer is not the dealer of this table.")]
    DealerNotSigner,
    #[msg("The provided price feed account is not owned by the Pyth program.")]
    InvalidPriceFeedOwner,
    #[msg("The backend signer does not match the one in the authority config.")]
    BackendSignerMismatch,
    #[msg("The caller is not authorized to force a player action.")]
    UnauthorizedForceAction,


    // --- Ошибки состояния игры ---
    #[msg("The game is not in a state to accept bets.")]
    NotAcceptingBets,
    #[msg("The game is not in the correct state for the initial card deal.")]
    InvalidGameStateForDeal,
    #[msg("It is not currently the player's turn phase.")]
    NotPlayerTurnsState,
    #[msg("It is not the correct seat's turn to act.")]
    WrongSeatForTurn,
    #[msg("It is not the correct hand's turn to act.")]
    WrongHandForTurn,
    #[msg("It is not currently the dealer's turn.")]
    NotDealerTurnState,
    #[msg("The current round is not over yet.")]
    NotRoundOverState,
    #[msg("Cannot close the table while a game is active.")]
    CannotCloseTableActiveGame,
    #[msg("The deck is empty. This should not happen with proper reshuffling logic.")]
    DeckEmpty,
    #[msg("Not enough active players to start the deal.")]
    NotEnoughPlayers,
    #[msg("Player still has an active bet and cannot leave the table.")]
    PlayerHasActiveBet,
    #[msg("The turn timer has not been set for the current player.")]
    TurnTimerNotSet,
    #[msg("The player's turn time has not expired yet.")]
    TurnTimeNotExpired,

    // --- Ошибки действий игрока ---
    #[msg("The hand is in a final state (Stood, Busted, Blackjack) and cannot be acted upon.")]
    HandActionOnFinalizedHand,
    #[msg("Cannot double down when the hand does not have exactly two cards.")]
    CannotDoubleNotTwoCards,
    #[msg("Cannot split a hand that has already been split.")]
    CannotSplitAlreadySplit,
    #[msg("Cannot split when the hand does not have exactly two cards.")]
    CannotSplitNotTwoCards,
    #[msg("Cannot split a hand whose cards do not have matching ranks.")]
    CannotSplitRanksMismatch,

    // --- Ошибки расчетов и финансов ---
    #[msg("The provided USDC token account does not match the required USDC mint.")]
    UsdcMintMismatch,
    #[msg("The provided token mint for the bet does not match the hand's required mint.")]
    BetTokenMintMismatch,
    #[msg("The calculated USD value of the bet does not match the provided value within the allowed slippage.")]
    PayoutCalculationMismatch,
    #[msg("An arithmetic overflow occurred during a calculation.")]
    ArithmeticOverflow,
    #[msg("Minimum bet cannot be zero.")]
    MinBetIsZero,
    #[msg("Insufficient funds to double down.")]
    InsufficientFundsForDoubleDown,
    #[msg("Insufficient funds to split.")]
    InsufficientFundsForSplit,
    #[msg("The total value of tokens in the bank is less than the dealer's locked collateral.")]
    InsufficientBankValue,
    #[msg("The Pyth price feed is stale and cannot be used.")]
    PriceFeedStale,
    #[msg("Cannot find the specified player at the table.")]
    CannotFindPlayerSeat,
    #[msg("Invalid hand index provided.")]
    InvalidHandIndex,
    #[msg("Player public key does not match the one in the seat.")]
    PlayerMismatch,
    #[msg("Hand cards provided by backend do not match on-chain state.")]
    HandCardsMismatch,
    #[msg("Game outcome provided by backend does not match on-chain calculation.")]
    OutcomeMismatch,
    #[msg("Cannot close table because there are still funds in one of the escrow accounts.")]
    TableHasActiveEscrow,
    #[msg("Attempted to withdraw a token that is not tracked in the dealer's profit.")]
    TokenMintNotInProfitTracker,
    #[msg("A commitment for the next shuffle must be provided when the deck is low.")]
    NextShuffleCommitmentRequired,
    #[msg("A shuffle is required, but no commitment for the nonce was found.")]
    ShuffleCommitmentMissing,
    #[msg("The revealed nonce for shuffling does not match the commitment.")]
    ShuffleCommitmentInvalid,
    #[msg("A shuffle is required, but no nonce was provided.")]
    ShuffleNonceRequired,
}
