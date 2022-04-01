use thiserror::Error;
use num_derive::FromPrimitive;
use solana_program::{
    decode_error::DecodeError,
    msg,
    program_error::{PrintProgramError, ProgramError},
};

#[derive(Clone, Debug, Eq, Error, FromPrimitive, PartialEq)]
pub enum BetError {
    // Invalid instruction
    #[error("Invalid Instruction")]
    InvalidInstruction,

    // Unauthorized account
    #[error("Incorrect signer")]
    IncorrectSigner,

    // Not rent exempt
    #[error("State account not rent exempt")]
    NotRentExempt,

    // Invalid mint
    #[error("Invalid mint")]
    InvalidMint,

    // Expected amount mismatch - wrong number of tokens in temporary token account
    #[error("Expected amount mismatch")]
    ExpectedAmountMismatch,

    // Unauthorized account
    #[error("Unauthorized account")]
    UnauthorizedAccount,

    // Incorrect account owner
    #[error("Incorrect account owner")]
    IncorrectOwner,

    // Account is not token account
    #[error("Account is not token account")]
    IsNotTokenAccount,

    // Account already initialized
    #[error("Account already initialized")]
    AccountAlreadyInitialized,

    // Invalid accounts
    #[error("Invalid accounts")]
    InvalidAccounts,

    // Invalid tournament account
    #[error("Invalid bet account")]
    InvalidBetAccount,

    // Invalid system program
    #[error("Invalid system program")]
    InvalidSystemProgram,

    // Amount overflow transferring lamports
    #[error("Amount overflow transferring lamports")]
    AmountOverflow,

    // Amount underflow transferring lamports
    #[error("Amount underflow transferring lamports")]
    AmountUnderflow,

    // Data type mismatch
    #[error("Data type mismatch")]
    DataTypeMismatch,

    // Invalid price account
    #[error("Invalid price account")]
    InvalidPriceAccount,

    // Invalid account input
    #[error("Invalid price account")]
    InvalidAccountInput,

    // Invalid oracle config
    #[error("Invalid oracle config")]
    InvalidOracleConfig,

    // No Payment Mint Given
    #[error("No payment mint given")]
    NoPaymentMintGiven,

    // Bet no longer valid
    #[error("Bet no longer valid")]
    BetNoLongerValid,

    // Invalid odds
    #[error("Invalid odds")]
    InvalidOdds,

    // Bet cancelled
    #[error("Bet cancelled")]
    BetCancelled,

    // Bet already finalized
    #[error("Bet already finalized")]
    BetFinalized,

    // Before bet expiry time
    #[error("Before expiry time")]
    BeforeExpiryTime,
}

impl PrintProgramError for BetError {
    fn print<E>(&self) {
        msg!(&self.to_string());
    }
}

impl From<BetError> for ProgramError {
    fn from(e: BetError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

impl<T> DecodeError<T> for BetError {
    fn type_of() -> &'static str {
        "Bet Error"
    }
}