use anchor_lang::prelude::*;

#[error_code]
pub enum TwentyOneError {
    #[msg("Arithmetic overflow")]
    ArithmeticOverflow,
    #[msg("Player does not have an active bet")]
    PlayerHasActiveBet,
    #[msg("Not currently accepting bets")]
    NotAcceptingBets,
    #[msg("Pyth price feed is stale")]
    PriceFeedStale,
    #[msg("Price feed is not available or not currently trading")]
    PriceFeedUnavailable,
    #[msg("Calculated payout does not match expected payout")]
    PayoutCalculationMismatch,
    #[msg("Invalid game state for dealing cards")]
    InvalidGameStateForDeal,
    #[msg("Not enough players to start the deal")]
    NotEnoughPlayers,
    #[msg("Invalid hand index provided")]
    InvalidHandIndex,
    #[msg("Action cannot be performed on a finalized hand (Stood, Busted, etc.)")]
    HandActionOnFinalizedHand,
    #[msg("Cannot double down when not having exactly two cards")]
    CannotDoubleNotTwoCards,
    #[msg("The token mint of the bet does not match the provided account")]
    BetTokenMintMismatch,
    #[msg("Insufficient funds to double down")]
    InsufficientFundsForDoubleDown,
    #[msg("Cannot split a hand that has already been split")]
    CannotSplitAlreadySplit,
    #[msg("Cannot split when not having exactly two cards")]
    CannotSplitNotTwoCards,
    #[msg("Cannot split cards with mismatched ranks")]
    CannotSplitRanksMismatch,
    #[msg("Insufficient funds to split")]
    InsufficientFundsForSplit,
    #[msg("It is not the dealer's turn")]
    NotDealerTurnState,
    #[msg("A payout instruction is missing for a winning hand")]
    PayoutInstructionMissing,
    #[msg("Payout instruction coordinates do not match expected seat/hand index")]
    PayoutInstructionMismatch,
    #[msg("Player token account not found in remaining accounts")]
    PlayerTokenAccountNotFound,
    #[msg("Pyth price feed account not found in remaining accounts")]
    EscrowBumpNotFound,
    #[msg("Withdrawal would leave insufficient value in the bank")]
    InsufficientBankValue,
    #[msg("Cannot close a table with an active game")]
    CannotCloseTableActiveGame,
    #[msg("Provided escrow account is not valid for this table")]
    InvalidEscrowAccountForClosure,
    #[msg("Dealer profit account owner mismatch")]
    DealerProfitAccountOwnerMismatch,
    #[msg("Owner fee account owner mismatch")]
    OwnerFeeAccountOwnerMismatch,
    #[msg("It is not the players' turn phase")]
    NotPlayerTurnsState,
    #[msg("It is not this player's turn to act")]
    NotThisPlayerTurn,
    #[msg("Turn timer has not been set")]
    TurnTimerNotSet,
    #[msg("Player turn time has not yet expired")]
    TurnTimeNotExpired,
    #[msg("The specified seat is not taken by any player")]
    SeatNotTaken,
    #[msg("The deck is out of cards")]
    DeckEmpty,
    #[msg("Cannot find a seat for the given player pubkey")]
    CannotFindPlayerSeat,
    #[msg("Token mint not found in dealer's profit tracker")]
    TokenMintNotInProfitTracker,
    #[msg("Table name is too short")]
    TableNameTooShort,
    #[msg("Table name is too long")]
    TableNameTooLong,
    #[msg("Table name contains invalid characters")]
    TableNameInvalidChars,
    #[msg("Table name has leading or trailing hyphens")]
    TableNameInvalidHyphenPlacement,
    #[msg("Table name has consecutive hyphens")]
    TableNameConsecutiveHyphens,
    #[msg("Signer is not the dealer for this table")]
    DealerNotSigner,
    #[msg("Signer is not the player at the specified seat index")]
    PlayerNotAtSeatIndex,
    #[msg("It is not this seat's turn")]
    WrongSeatForTurn,
    #[msg("It is not this hand's turn")]
    WrongHandForTurn,
    #[msg("The signer is not authorized to perform this action")]
    Unauthorized,
    #[msg("The backend signer is not authorized")]
    UnauthorizedBackendSigner,
    #[msg("Provided token account is not owned by the signer")]
    PlayerNotSigner,
    #[msg("Provided USDC token account does not match the required mint")]
    UsdcMintMismatch,
    #[msg("Failed to parse a pubkey from string")]
    PubkeyParseError,
    #[msg("Minimum bet cannot be zero")]
    MinBetIsZero,
    #[msg("Invalid seat index")]
    InvalidSeatIndex,
    #[msg("Seat is already taken")]
    SeatTaken,
    #[msg("Owner fee token account has the wrong mint")]
    OwnerFeeAccountMintMismatch,
    #[msg("Dealer profit token account has the wrong mint")]
    DealerProfitAccountMintMismatch,
    #[msg("Round is not over yet")]
    NotRoundOverState,
    #[msg("Price feed not found in the provided accounts")]
    PriceFeedNotFound,
    #[msg("Provided price feed account has invalid owner")]
    InvalidPriceFeedOwner,
    #[msg("Table still has active escrow funds")]
    TableHasActiveEscrow,
    #[msg("Player mismatch in backend results")]
    PlayerMismatch,
    #[msg("Hand not found for the specified indices")]
    HandNotFound,
    #[msg("Hand cards mismatch between backend and contract")]
    HandCardsMismatch,
    #[msg("Hand score mismatch between backend and contract")]
    HandScoreMismatch,
    #[msg("Outcome mismatch between backend calculation and contract")]
    OutcomeMismatch,
    #[msg("Invalid price feed index in remaining accounts")]
    InvalidPriceFeedIndex,
    #[msg("Duplicate account index detected in remaining accounts")]
    DuplicateAccountIndex,
}
