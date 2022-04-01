use solana_program::{
    pubkey::Pubkey,
    account_info::AccountInfo,
    program_error::ProgramError
};
use borsh::{BorshSerialize, BorshDeserialize};
use crate::{
    utils::try_from_slice_checked
};

// BET DIRECTIONS
#[repr(C)]
#[derive(BorshSerialize, BorshDeserialize, PartialEq, Debug, Clone, Copy)]
pub enum Direction {
    Above,
    Below
}

// CANCEL BET CONDITIONS
#[repr(C)]
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct CancelCondition {
    pub below_price: i64,
    pub above_price: i64,
    pub time: i64
}

// BET ACCOUNT
pub const MAX_BET_DATA_LENGTH: usize = 1 + 32 + 32 + 1 + 32 + 2 + 8 + 32 + 32 + 8 + 1 + 8 + 8 + 8 + 8 + 8;

#[repr(C)]
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub struct Bet {
    pub is_initialized: bool,
    pub betting_market: Pubkey,
    pub creator_main_account: Pubkey, 
    pub creator_payment_account: Pubkey,
    pub bet_escrow_account: Pubkey,
    pub odds: i64,
    pub bet_size: u64,
    pub pyth_oracle_product_account: Pubkey,
    pub pyth_oracle_price_account: Pubkey,
    pub expiration_time: i64,
    pub bet_direction: Direction,
    pub bet_price: i64, // price that asset must be above/below at time of bet expiration
    pub start_price: i64, // price when bet is created
    pub cancel_condition: CancelCondition,
    pub variable_odds: Option<i64>,
    pub total_amount_accepted: u64,
    pub cancelled: bool
}

impl Bet {
    pub fn from_account_info(a: &AccountInfo) -> Result<Bet, ProgramError> {
        let bet: Bet = try_from_slice_checked(&a.data.borrow_mut(), MAX_BET_DATA_LENGTH)?;
        Ok(bet)
    }
}

// BETTING MARKET - we create a market for each coin that can be used for bets e.g. paying with SOL uses the SOL betting market
// ensures the correct oracle program and fee commission account is used

pub const MAX_BETTING_MARKET_DATA_LEN: usize = 32 + 32 + 1 + 32 + 32;

#[repr(C)]
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub struct BettingMarket {
    pub owner: Pubkey,
    pub fee_commission_account: Pubkey,
    pub sol_payment: bool, // if true, market uses SOL for payment
    pub payment_mint: Option<Pubkey>, // if not using SOL, then need mint of token
    pub pyth_program_id: Pubkey
}

impl BettingMarket {
    pub fn from_account_info(a: &AccountInfo) -> Result<BettingMarket, ProgramError> {
        let market: BettingMarket = try_from_slice_checked(&a.data.borrow_mut(), MAX_BETTING_MARKET_DATA_LEN)?;
        Ok(market)
    }
}

// ACCEPTED BET
pub const MAX_ACCEPTED_BET_DATA_LEN: usize = 32 + 32 + 32 + 8 + 8;

#[repr(C)]
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub struct AcceptedBet {
    pub bet: Pubkey,
    pub acceptor_main_account: Pubkey,
    pub acceptor_payment_account: Pubkey,
    pub odds: i64,
    pub bet_size: u64
}

impl AcceptedBet {
    pub fn from_account_info(a: &AccountInfo) -> Result<AcceptedBet, ProgramError> {
        let bet: AcceptedBet = try_from_slice_checked(&a.data.borrow_mut(), MAX_ACCEPTED_BET_DATA_LEN)?;
        Ok(bet)
    }
}