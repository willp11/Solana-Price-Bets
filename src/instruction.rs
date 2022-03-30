use borsh::{BorshSerialize, BorshDeserialize};
use solana_program::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    sysvar,
    system_program
};

#[repr(C)]
#[derive(BorshSerialize, BorshDeserialize, PartialEq, Debug, Clone)]
/// Args for create bet
pub struct CreateBetArgs {
    sol_payment: bool, // true is paid with SOL, false is paid with a token
    bet_size: u64,
    odds: u16, // the odds given for the bet, e.g. even odds = 2.00 = 200
    expiration_time: i64, // the time at which the bet expires
    bet_direction: String, // "above" / "below"
    bet_price: i64, // the price the asset must be above/below at expiration time
    cancel_price: i64, // the price at which the bet is no longer valid and thus can no longer be accepted
    cancel_time: i64, // the time at which the bet is no longer valid and thus can no longer be accepted
    variable_odds: i64, // the amount price must change for odds to increase by 0.01
}

/// Instructions supported by the YoYo Bet program
#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub enum BetInstruction {
    // [signer] creator_main_account
    // [writable] creator_payment_account
    // [writable] bet_state_account
    // [writable] bet_escrow_account
    // [] pyth_oracle_product_account
    // [] pyth_oracle_price_account
    // [] rent_sysvar
    // [] system_program
    // [] token_program
    CreateBet(CreateBetArgs),
}

/// Creates a CreateBet Instruction
#[allow(clippy::too_many_arguments)]
pub fn create_bet(
    program_id: Pubkey,
    creator_main_account: Pubkey,
    creator_payment_account: Pubkey,
    bet_state_account: Pubkey,
    bet_escrow_account: Pubkey,
    pyth_oracle_product_account: Pubkey,
    pyth_oracle_price_account: Pubkey,
    sol_payment: bool,
    bet_size: u64,
    odds: u16,
    expiration_time: i64,
    bet_direction: String,
    bet_price: i64,
    cancel_price: i64,
    cancel_time: i64,
    variable_odds: i64,
) -> Instruction {
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(creator_main_account, true),
            AccountMeta::new(creator_payment_account, false),
            AccountMeta::new(bet_state_account, false),
            AccountMeta::new(bet_escrow_account, false),
            AccountMeta::new_readonly(pyth_oracle_product_account, false),
            AccountMeta::new_readonly(pyth_oracle_price_account, false),
            AccountMeta::new_readonly(sysvar::rent::id(), false),
            AccountMeta::new_readonly(spl_token::ID, false),
            AccountMeta::new_readonly(system_program::id(), false)
        ],
        data: BetInstruction::CreateBet(CreateBetArgs {
            sol_payment,
            bet_size,
            odds,
            expiration_time,
            bet_direction,
            bet_price,
            cancel_price,
            cancel_time,
            variable_odds,
        })
        .try_to_vec()
        .unwrap(),
    }
}