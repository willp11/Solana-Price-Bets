use solana_program::{
    pubkey::Pubkey,
    account_info::AccountInfo,
    program_error::ProgramError
};
use borsh::{BorshSerialize, BorshDeserialize};
use crate::{
    utils::try_from_slice_checked
};

// BET ACCOUNT
pub const BET_DIRECTION_DATA_LENGTH: usize = 5;
pub const MAX_BET_DATA_LENGTH: usize = 1 + 32 + 32 + 1 + 32 + 2 + 8 + 32 + 32 + 8 + BET_DIRECTION_DATA_LENGTH + 8 + 8 + 8 + 8 + 8;

#[repr(C)]
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub struct BetAccount {
    pub is_initialized: bool,
    pub creator_main_account: Pubkey, 
    pub creator_payment_account: Pubkey,
    pub sol_payment: bool,
    pub payment_mint: Pubkey, //optional
    pub odds: u16,
    pub bet_size: u64,
    pub pyth_oracle_product_account: Pubkey,
    pub pyth_oracle_price_account: Pubkey,
    pub expiration_time: i64,
    pub bet_direction: String, // "above" / "below"
    pub bet_price: i64,
    pub cancel_price: i64,
    pub cancel_time: i64,
    pub variable_odds: i64,
    pub total_amount_accepted: u64
}

impl BetAccount {
    pub fn from_account_info(a: &AccountInfo) -> Result<BetAccount, ProgramError> {
        let bet: BetAccount = try_from_slice_checked(&a.data.borrow_mut(), MAX_BET_DATA_LENGTH)?;

        Ok(bet)
    }
}