use borsh::{BorshSerialize, BorshDeserialize};
use solana_program::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    sysvar,
    system_program
};

use crate::{
    state::{CancelCondition, Direction}
};

#[repr(C)]
#[derive(BorshSerialize, BorshDeserialize, PartialEq, Debug, Clone)]
/// Args for init betting market
pub struct InitBettingMarketArgs {
    pub sol_payment: bool, // true is paid with SOL, false is paid with a token
    pub payment_mint: Option<Pubkey>
}

#[repr(C)]
#[derive(BorshSerialize, BorshDeserialize, PartialEq, Debug, Clone)]
/// Args for create bet
pub struct CreateBetArgs {
    pub bet_size: u64,
    pub odds: i64, // the odds given for the bet, e.g. even odds = 2.00 = 200
    pub expiration_time: i64, // the time at which the bet expires
    pub bet_direction: Direction, // "above" / "below"
    pub bet_price: i64, // the price the asset must be above/below at expiration time
    pub cancel_condition: CancelCondition,
    pub variable_odds: Option<i64>, // the amount price must change for odds to increase by 1
}

#[repr(C)]
#[derive(BorshSerialize, BorshDeserialize, PartialEq, Debug, Clone)]
/// Args for accept bet
pub struct AcceptBetArgs {
    // how much of the original bet is being bet. (bet_size != payment amount for the acceptor). 
    // E.g. original bet size 200, odds 1.50. Total payments = 200*1.50 = 300. (acceptors must pay 300 - 200 = 100 total)
    // bet_size = 100, so is accepting half the original bet, so this acceptor pays 50.
    pub bet_size: u64,
}

/// Instructions supported by the YoYo Bet program
#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub enum BetInstruction {
    // [signer] owner_account
    // [writable] betting_market_account
    // [] commission_fee_account
    // [] pyth_program
    InitBettingMarket(InitBettingMarketArgs),

    // [signer] creator_main_account
    // [writable] creator_payment_account
    // [writable] bet_state_account
    // [writable] bet_escrow_account
    // [] betting_market_account
    // [] pyth_oracle_product_account
    // [] pyth_oracle_price_account
    // [] rent_sysvar
    // [] system_program
    // [] token_program
    CreateBet(CreateBetArgs),

    // [signer] acceptor_main_account
    // [writable] acceptor_payment_account
    // [writable] bet_state_account
    // [writable] bet_escrow_account
    // [writable] accepted_bet_state_account
    // [writable] accepted_bet_escrow_account
    // [] betting_market_account
    // [] pyth_oracle_price_account
    // [] rent_sysvar
    // [] system_program
    // [] token_program
    // [] clock_sysvar
    // [] PDA - has transfer authority for escrow token accounts
    AcceptBet(AcceptBetArgs),

    // [signer] creator_main_account
    // [writable] creator_payment_account
    // [writable] bet_state_account
    // [writable] bet_escrow_account
    // [] betting_market_account
    // [] system_program
    // [] token_program
    // [] PDA
    CancelBet(),

    // [signer] finalizer_main_account
    // [writable] finalizer_payment_account
    // [writable] commission_fee_account
    // [] bet_state_account
    // [] accepted_bet_state_account
    // [writable] accepted_bet_escrow_account
    // [writable] creator_payment_account
    // [writable] acceptor_payment_account
    // [] pyth_price_account
    // [] betting_market_account
    // [] system_program
    // [] token_program
    // [] PDA
    // [] clock_sysvar
    FinalizeBet(),
}

/// Creates a InitBettingMarket Instruction
pub fn init_betting_market(
    program_id: Pubkey,
    owner_account: Pubkey,
    betting_market_account: Pubkey,
    commission_fee_account: Pubkey,
    pyth_program: Pubkey,
    sol_payment: bool,
    payment_mint: Option<Pubkey>
) -> Instruction {
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(owner_account, true),
            AccountMeta::new(betting_market_account, false),
            AccountMeta::new_readonly(commission_fee_account, false),
            AccountMeta::new_readonly(pyth_program, false)
        ],
        data: BetInstruction::InitBettingMarket(InitBettingMarketArgs {
            sol_payment: sol_payment,
            payment_mint: payment_mint
        })
        .try_to_vec()
        .unwrap()
    }
}

/// Creates a CreateBet Instruction
#[allow(clippy::too_many_arguments)]
pub fn create_bet(
    program_id: Pubkey,
    creator_main_account: Pubkey,
    creator_payment_account: Pubkey,
    bet_state_account: Pubkey,
    bet_escrow_account: Pubkey,
    betting_market_account: Pubkey,
    pyth_oracle_product_account: Pubkey,
    pyth_oracle_price_account: Pubkey,
    bet_size: u64,
    odds: i64,
    expiration_time: i64,
    bet_direction: Direction,
    bet_price: i64,
    cancel_condition: CancelCondition,
    variable_odds: Option<i64>,
) -> Instruction {
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(creator_main_account, true),
            AccountMeta::new(creator_payment_account, false),
            AccountMeta::new(bet_state_account, false),
            AccountMeta::new(bet_escrow_account, false),
            AccountMeta::new_readonly(betting_market_account, false),
            AccountMeta::new_readonly(pyth_oracle_product_account, false),
            AccountMeta::new_readonly(pyth_oracle_price_account, false),
            AccountMeta::new_readonly(sysvar::rent::id(), false),
            AccountMeta::new_readonly(spl_token::ID, false),
            AccountMeta::new_readonly(system_program::id(), false)
        ],
        data: BetInstruction::CreateBet(CreateBetArgs {
            bet_size,
            odds,
            expiration_time,
            bet_direction,
            bet_price,
            cancel_condition,
            variable_odds,
        })
        .try_to_vec()
        .unwrap(),
    }
}

/// Creates a CreateBet Instruction
#[allow(clippy::too_many_arguments)]
pub fn accept_bet(
    program_id: Pubkey,
    acceptor_main_account: Pubkey,
    acceptor_payment_account: Pubkey,
    bet_state_account: Pubkey,
    bet_escrow_account: Pubkey,
    accepted_bet_state_account: Pubkey,
    accepted_bet_escrow_account: Pubkey,
    betting_market_account: Pubkey,
    pyth_oracle_price_account: Pubkey,
    bet_size: u64,
) -> Instruction {
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(acceptor_main_account, true),
            AccountMeta::new(acceptor_payment_account, false),
            AccountMeta::new(bet_state_account, false),
            AccountMeta::new(bet_escrow_account, false),
            AccountMeta::new(accepted_bet_state_account, false),
            AccountMeta::new(accepted_bet_escrow_account, false),
            AccountMeta::new_readonly(betting_market_account, false),
            AccountMeta::new_readonly(pyth_oracle_price_account, false),
            AccountMeta::new_readonly(sysvar::rent::id(), false),
            AccountMeta::new_readonly(spl_token::ID, false),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new_readonly(sysvar::clock::id(), false),
        ],
        data: BetInstruction::AcceptBet(AcceptBetArgs {
            bet_size,
        })
        .try_to_vec()
        .unwrap(),
    }
}