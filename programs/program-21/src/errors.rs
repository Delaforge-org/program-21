// programs/blackjack_anchor/src/errors.rs

use anchor_lang::prelude::*;

#[error_code]
pub enum BlackjackError {
    #[msg("Table name is too short.")]
    TableNameTooShort,
    #[msg("Table name is too long.")]
    TableNameTooLong,
    #[msg("Table name contains invalid characters.")]
    TableNameInvalidChars,
    #[msg("Table name cannot start or end with a hyphen.")]
    TableNameInvalidHyphenPlacement,
    #[msg("Table name cannot have consecutive hyphens.")]
    TableNameConsecutiveHyphens,
    #[msg("Max players must be between 1 and 6.")]
    InvalidMaxPlayers,
    #[msg("Min bet must be less than or equal to max bet.")]
    MinBetExceedsMaxBet,
    #[msg("Min bet must be greater than 0.")]
    MinBetIsZero,
    #[msg("Collateral calculation resulted in overflow.")]
    CollateralOverflow,
    #[msg("Game session account is not active or does not exist.")]
    GameSessionNotActive,
    #[msg("Seat index is out of bounds.")]
    InvalidSeatIndex,
    #[msg("Seat is already taken.")]
    SeatTaken,
    #[msg("Seat is empty.")]
    SeatEmpty,
    #[msg("Player not found at specified seat.")]
    PlayerNotFoundAtSeat,
    #[msg("Player has an active bet or unresolved hand.")]
    PlayerHasActiveBet,
    #[msg("Game is not in AcceptingBets state.")]
    NotAcceptingBets,
    #[msg("Dealer has stopped accepting new bets for new rounds.")]
    BetsAcceptanceStoppedByDealer,
    #[msg("Bet value is below minimum table bet.")]
    BetBelowMin,
    #[msg("Bet value is above maximum table bet.")]
    BetAboveMax,
    #[msg("Player is not at the specified seat index.")]
    PlayerNotAtSeatIndex,
    #[msg("Player has no bet placed for this round.")]
    PlayerHasNoBet,
    #[msg("Game state is not Dealing or invalid for this action.")]
    InvalidGameStateForDeal,
    #[msg("Not enough players to start the round.")]
    NotEnoughPlayers,
    #[msg("Game state is not PlayerTurns.")]
    NotPlayerTurnsState,
    #[msg("It is not this player's turn.")]
    NotPlayerTurn,
    #[msg("Invalid hand index for player.")]
    InvalidHandIndex,
    #[msg("Hand is not in Playing state.")]
    HandNotPlaying,
    #[msg("Deck is empty or insufficient cards.")]
    DeckEmpty,
    #[msg("Cannot double down, player does not have exactly two cards.")]
    CannotDoubleNotTwoCards,
    #[msg("Cannot double down, not enough funds.")]
    InsufficientFundsForDoubleDown,
    #[msg("Cannot split, player does not have exactly two cards.")]
    CannotSplitNotTwoCards,
    #[msg("Cannot split, card ranks do not match.")]
    CannotSplitRanksMismatch,
    #[msg("Cannot split, player has already split this hand.")]
    CannotSplitAlreadySplit,
    #[msg("Cannot split, not enough funds.")]
    InsufficientFundsForSplit,
    #[msg("Game state is not DealerTurn.")]
    NotDealerTurnState,
    #[msg("Game state is not RoundOver.")]
    NotRoundOverState,
    #[msg("No profit to withdraw for the specified token or at all.")]
    NoProfitToWithdraw,
    #[msg("Cannot close table, active players or unresolved bets exist.")]
    CannotCloseTableActiveGame,
    #[msg("Arithmetic overflow/underflow occurred.")]
    ArithmeticOverflow,
    #[msg("The provided token mint for withdrawal is not in profit tracker.")]
    TokenMintNotInProfitTracker,
    #[msg("Player already has a hand. Cannot re-initialize.")]
    PlayerHandAlreadyExists,
    #[msg("Player hand not found for action.")]
    PlayerHandNotFound,
    #[msg("Cannot hit on a hand that is not Playing.")]
    CannotHitHandNotPlaying,
    #[msg("Cannot stand on a hand that is not Playing.")]
    CannotStandHandNotPlaying,
    #[msg("Dealer must be the signer for this action.")]
    DealerNotSigner,
    #[msg("Player must be the signer for this action.")]
    PlayerNotSigner,
    #[msg("Payout calculation error.")]
    PayoutError,
    #[msg("Failed to parse pubkey from string.")]
    PubkeyParseError,
    #[msg("The chosen seat is not for the current turn player.")]
    WrongSeatForTurn,
    #[msg("The chosen hand is not for the current turn player.")]
    WrongHandForTurn,
    #[msg("Cannot perform action, player has Blackjack or is Busted/Stood.")]
    PlayerHandFinalized,
    #[msg("Token mint for bet does not match hand's token mint.")]
    BetTokenMintMismatch,
    #[msg("Invalid number of cards to deal for split.")]
    InvalidSplitDeal,
    #[msg("Cannot find player seat for pubkey.")]
    CannotFindPlayerSeat,
    #[msg("USDC Mint mismatch.")]
    UsdcMintMismatch,
    #[msg("Owner fee recipient account mint mismatch.")]
    OwnerFeeAccountMintMismatch,
    #[msg("Owner fee recipient account owner mismatch.")]
    OwnerFeeAccountOwnerMismatch,
    #[msg("Dealer profit account mint mismatch.")]
    DealerProfitAccountMintMismatch,
    #[msg("Dealer profit account owner mismatch.")]
    DealerProfitAccountOwnerMismatch,
    #[msg("Player token account mint mismatch for payout.")]
    PlayerPayoutAccountMintMismatch,
    #[msg("Player token account owner mismatch for payout.")]
    PlayerPayoutAccountOwnerMismatch,
    #[msg("Cannot perform action on this hand, it is already finalized (Blackjack, Busted, Stood).")]
    HandActionOnFinalizedHand,
    #[msg("The game session account's authority for the SPL token escrow does not match.")]
    EscrowAuthorityMismatch,
    #[msg("Remaining accounts are not structured as expected for payouts.")]
    InvalidRemainingAccountsForPayout,
}