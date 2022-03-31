use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    pubkey::Pubkey,
    program_pack::{Pack},
    sysvar::{rent::Rent, Sysvar},
    program::{invoke, invoke_signed},
    clock::{Clock},
    system_program::{check_id},
    system_instruction,
    program_error::ProgramError
};

use spl_token::state::Account as TokenAccount;

use crate::{
    instruction::BetInstruction,
    error::BetError,
    utils::PREFIX,
    utils::create_or_allocate_account_raw,
    utils::puffed_out_string,
    state::{BettingMarket, Bet},
    pyth
};

use std::convert::TryInto;
use borsh::{BorshSerialize, BorshDeserialize};

use pyth_client::{
    Product,
    Price,
    PriceConf,
    load_price,
    load_product
};

pub fn process_instruction<'a>(
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
    input: &[u8],
) -> ProgramResult {
    let instruction = BetInstruction::try_from_slice(input)?;
    match instruction {
        BetInstruction::InitBettingMarket(args) => {
            msg!("Instruction: Init Betting Market");
            process_init_betting_market(
                program_id, 
                accounts, 
                args.sol_payment, 
                args.payment_mint
            )
        },
        BetInstruction::CreateBet(args) => {
            msg!("Instruction: Create Bet");
            process_create_bet(
                program_id,
                accounts,
                args.bet_size,
                args.odds,
                args.expiration_time,
                args.bet_direction,
                args.bet_price,
                args.cancel_price,
                args.cancel_time,
                args.variable_odds,
            )
        },
    }
}

pub fn process_init_betting_market<'a>(
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
    sol_payment: bool,
    payment_mint: Option<Pubkey>
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let owner_account_info = next_account_info(account_info_iter)?;
    let betting_market_account_info = next_account_info(account_info_iter)?;
    let commission_fee_account_info = next_account_info(account_info_iter)?;
    let pyth_program = next_account_info(account_info_iter)?;

    if !owner_account_info.is_signer {
        return Err(BetError::IncorrectOwner.into());
    }

    let mut betting_market_account = BettingMarket::from_account_info(betting_market_account_info)?;

    if sol_payment == false {
        if let Some(mint) = payment_mint {
            betting_market_account.payment_mint = Some(mint);
        } else {
            return Err(BetError::NoPaymentMintGiven.into());
        }
    }
    betting_market_account.owner = *owner_account_info.key;
    betting_market_account.sol_payment = sol_payment;
    betting_market_account.fee_commission_account = *commission_fee_account_info.key;
    betting_market_account.pyth_program_id = *pyth_program.key;

    Ok(())
}

pub fn process_create_bet<'a>(
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
    bet_size: u64,
    odds: u16,
    expiration_time: i64,
    bet_direction: String,
    bet_price: i64,
    cancel_price: i64,
    cancel_time: i64,
    variable_odds: i64,
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let creator_main_account_info = next_account_info(account_info_iter)?;
    let creator_payment_account_info = next_account_info(account_info_iter)?;
    let bet_state_account_info = next_account_info(account_info_iter)?;
    let bet_escrow_account_info = next_account_info(account_info_iter)?;
    let betting_market_account_info = next_account_info(account_info_iter)?;
    let pyth_oracle_product_account_info = next_account_info(account_info_iter)?;
    let pyth_oracle_price_account_info = next_account_info(account_info_iter)?;
    let rent = &Rent::from_account_info(next_account_info(account_info_iter)?)?;
    let token_program_account_info = next_account_info(account_info_iter)?;
    spl_token::check_program_account(token_program_account_info.key)?;
    let system_program_account_info = next_account_info(account_info_iter)?;
    if check_id(system_program_account_info.key) == false {
        return Err(BetError::InvalidSystemProgram.into());
    }

    // check creator_account_info is the tx signer
    if !creator_main_account_info.is_signer {
        return Err(BetError::IncorrectSigner.into());
    }

    // check program is owner of the bet_state_account_info
    if bet_state_account_info.owner != program_id {
        return Err(BetError::IncorrectOwner.into());
    }

    // check bet_state_account_info has enough lamports to be rent exempt
    if !rent.is_exempt(bet_state_account_info.lamports(), bet_state_account_info.data_len()) {
        return Err(BetError::NotRentExempt.into());
    }

    // unpack the bet_state_account_info
    let mut bet_state_account = Bet::from_account_info(&bet_state_account_info)?;
    // unpack the betting_market_account_info
    let betting_market_account = BettingMarket::from_account_info(&betting_market_account_info)?;

    // check if bet payment is SOL or a token
    if betting_market_account.sol_payment == true {
        // if yes, check that the program id is owner of the bet escrow account
        if bet_escrow_account_info.owner != program_id {
            return Err(BetError::IncorrectOwner.into());
        }
    } else {
        // if no, check bet escrow account and creator payment account are token accounts 
        if *bet_escrow_account_info.owner != spl_token::ID || *creator_payment_account_info.owner != spl_token::ID {
            return Err(BetError::IsNotTokenAccount.into());
        }
        // unpack the token account data
        let bet_escrow_account = TokenAccount::unpack_from_slice(&bet_escrow_account_info.data.borrow())?;
        let creator_payment_account = TokenAccount::unpack_from_slice(&creator_payment_account_info.data.borrow())?;

        // check the escrow and payment accounts have the same mint and that it is same mint as in the betting market account
        if let Some(payment_mint) = betting_market_account.payment_mint {
            if bet_escrow_account.mint != creator_payment_account.mint || bet_escrow_account.mint != payment_mint {
                return Err(BetError::InvalidMint.into());
            }
        }

        // get the PDA account Pubkey (derived from the bet_escrow_account_info Pubkey and prefix "yoyobet")
        let bet_escrow_account_seeds = &[
            PREFIX.as_bytes(),
            bet_escrow_account_info.key.as_ref(),
        ];
        let (bet_escrow_account_pda, _bump_seed) = Pubkey::find_program_address(bet_escrow_account_seeds, program_id);

        
        // call token program to transfer ownership of bet escrow account to PDA
        let transfer_authority_change_ix = spl_token::instruction::set_authority(
            token_program_account_info.key,
            bet_escrow_account_info.key,
            Some(&bet_escrow_account_pda),
            spl_token::instruction::AuthorityType::AccountOwner,
            creator_main_account_info.key,
            &[&creator_main_account_info.key],
        )?;
        msg!("Calling the token program to transfer ownership authority to PDA...");
        invoke(
            &transfer_authority_change_ix,
            &[
                bet_escrow_account_info.clone(),
                creator_main_account_info.clone(),
                token_program_account_info.clone(),
            ],
        )?;
    }

    // check valid pyth keys
    validate_pyth_keys(
        &betting_market_account.pyth_program_id,
        pyth_oracle_product_account_info, 
        pyth_oracle_price_account_info
    )?;

    // check tournament state account hasn't already been initialized
    if bet_state_account.is_initialized == true {
        return Err(BetError::AccountAlreadyInitialized.into())
    }

    // write the data to state
    bet_state_account.is_initialized = true;
    bet_state_account.creator_main_account = *creator_main_account_info.key;
    bet_state_account.creator_payment_account = *creator_payment_account_info.key;
    bet_state_account.odds = odds;
    bet_state_account.bet_size = bet_size;
    bet_state_account.pyth_oracle_product_account = *pyth_oracle_product_account_info.key;
    bet_state_account.pyth_oracle_price_account = *pyth_oracle_price_account_info.key;
    bet_state_account.expiration_time = expiration_time;
    bet_state_account.bet_direction = bet_direction;
    bet_state_account.bet_price = bet_price;
    bet_state_account.cancel_price = cancel_price;
    bet_state_account.cancel_time = cancel_time;
    bet_state_account.variable_odds = variable_odds;
    bet_state_account.total_amount_accepted = 0;

    // pack the tournament_state_account
    bet_state_account.serialize(&mut &mut bet_state_account_info.data.borrow_mut()[..])?;
   
    Ok(())
}

/// validates pyth AccountInfos - Thank you Solend
#[inline(always)]
fn validate_pyth_keys(
    oracle_program_id: &Pubkey,
    pyth_product_info: &AccountInfo,
    pyth_price_info: &AccountInfo,
) -> ProgramResult {

    if oracle_program_id != pyth_product_info.owner {
        msg!("Pyth product account provided is not owned by the Pyth oracle program");
        return Err(BetError::InvalidOracleConfig.into());
    }
    if oracle_program_id != pyth_price_info.owner {
        msg!("Pyth price account provided is not owned by the Pyth oracle program");
        return Err(BetError::InvalidOracleConfig.into());
    }

    let pyth_product_data = pyth_product_info.try_borrow_data()?;
    let pyth_product = pyth::load::<pyth::Product>(&pyth_product_data)
        .map_err(|_| ProgramError::InvalidAccountData)?;
    if pyth_product.magic != pyth::MAGIC {
        msg!("Pyth product account provided is not a valid Pyth account");
        return Err(BetError::InvalidOracleConfig.into());
    }
    if pyth_product.ver != pyth::VERSION_2 {
        msg!("Pyth product account provided has a different version than expected");
        return Err(BetError::InvalidOracleConfig.into());
    }
    if pyth_product.atype != pyth::AccountType::Product as u32 {
        msg!("Pyth product account provided is not a valid Pyth product account");
        return Err(BetError::InvalidOracleConfig.into());
    }

    let pyth_price_pubkey_bytes: &[u8; 32] = pyth_price_info
        .key
        .as_ref()
        .try_into()
        .map_err(|_| BetError::InvalidAccountInput)?;
    if &pyth_product.px_acc.val != pyth_price_pubkey_bytes {
        msg!("Pyth product price account does not match the Pyth price provided");
        return Err(BetError::InvalidOracleConfig.into());
    }

    // let quote_currency = get_pyth_product_quote_currency(pyth_product)?;
    // if lending_market.quote_currency != quote_currency {
    //     msg!("Lending market quote currency does not match the oracle quote currency");
    //     return Err(BetError::InvalidOracleConfig.into());
    // }
    Ok(())
}