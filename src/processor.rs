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
    state::{BettingMarket, Bet, Direction, CancelCondition, AcceptedBet},
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
                args.cancel_condition,
                args.variable_odds,
            )
        },
        BetInstruction::AcceptBet(args) => {
            msg!("Instruction: Accept Bet");
            process_accept_bet(
                program_id,
                accounts,
                args.bet_size
            )
        },
        BetInstruction::CancelBet() => {
            msg!("Instruction: Cancel Bet");
            process_cancel_bet(
                program_id,
                accounts,
            )
        },
        BetInstruction::FinalizeBet() => {
            msg!("Instruction: Finalize Bet");
            process_finalize_bet(
                program_id,
                accounts
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

    // check owner signed tx
    if !owner_account_info.is_signer {
        return Err(BetError::IncorrectOwner.into());
    }

    // check program is owner of the bet_state_account_info
    if betting_market_account_info.owner != program_id {
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
    odds: i64,
    expiration_time: i64,
    bet_direction: Direction,
    bet_price: i64,
    cancel_condition: CancelCondition,
    variable_odds: Option<i64>,
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

        // check escrow account has enough lamports for the bet
        if bet_escrow_account_info.lamports() < bet_size {
            return Err(BetError::AmountUnderflow.into());
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

        // check escrow account has enough tokens for the bet
        if bet_escrow_account.amount < bet_size {
            return Err(BetError::AmountUnderflow.into());
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

    // get the price from oracle (used for variable odds)
    let pyth_price_data = pyth_oracle_price_account_info.try_borrow_data()?;
    let price_account: Price = *load_price( &pyth_price_data ).unwrap();
    let price: PriceConf = price_account.get_current_price().unwrap();

    // assert odds aren't less than 100
    if odds < 100 {
        return Err(BetError::InvalidOdds.into());
    }

    // write the data to state
    bet_state_account.is_initialized = true;
    bet_state_account.creator_main_account = *creator_main_account_info.key;
    bet_state_account.creator_payment_account = *creator_payment_account_info.key;
    bet_state_account.bet_escrow_account = *bet_escrow_account_info.key;
    bet_state_account.odds = odds;
    bet_state_account.bet_size = bet_size;
    bet_state_account.pyth_oracle_product_account = *pyth_oracle_product_account_info.key;
    bet_state_account.pyth_oracle_price_account = *pyth_oracle_price_account_info.key;
    bet_state_account.expiration_time = expiration_time;
    bet_state_account.bet_direction = bet_direction;
    bet_state_account.bet_price = bet_price;
    bet_state_account.start_price = price.price;
    bet_state_account.cancel_condition = cancel_condition;
    bet_state_account.variable_odds = variable_odds;
    bet_state_account.total_amount_accepted = 0;
    bet_state_account.cancelled = false;

    // pack the bet_state_account
    bet_state_account.serialize(&mut &mut bet_state_account_info.data.borrow_mut()[..])?;
   
    Ok(())
}

pub fn process_accept_bet<'a>(
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
    bet_size: u64,
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let acceptor_main_account_info = next_account_info(account_info_iter)?;
    let acceptor_payment_account_info = next_account_info(account_info_iter)?;
    let bet_state_account_info = next_account_info(account_info_iter)?;
    let bet_escrow_account_info = next_account_info(account_info_iter)?;
    let accepted_bet_state_account_info = next_account_info(account_info_iter)?;
    let accepted_bet_escrow_account_info = next_account_info(account_info_iter)?;
    let betting_market_account_info = next_account_info(account_info_iter)?;
    let pyth_oracle_price_account_info = next_account_info(account_info_iter)?;
    let rent = &Rent::from_account_info(next_account_info(account_info_iter)?)?;
    let token_program_account_info = next_account_info(account_info_iter)?;
    spl_token::check_program_account(token_program_account_info.key)?;
    let system_program_account_info = next_account_info(account_info_iter)?;
    if check_id(system_program_account_info.key) == false {
        return Err(BetError::InvalidSystemProgram.into());
    }
    let clock = &Clock::from_account_info(next_account_info(account_info_iter)?)?;
    let pda_account_info = next_account_info(account_info_iter)?;

    // check acceptor_main_account_info is the tx signer
    if !acceptor_main_account_info.is_signer {
        return Err(BetError::IncorrectSigner.into());
    }

    // check program is owner of the accepted_bet_state_account_info
    if accepted_bet_state_account_info.owner != program_id {
        return Err(BetError::IncorrectOwner.into());
    }

    // check accepted_bet_state_account_info has enough lamports to be rent exempt
    if !rent.is_exempt(accepted_bet_state_account_info.lamports(), accepted_bet_state_account_info.data_len()) {
        return Err(BetError::NotRentExempt.into());
    }

    // unpack the bet and betting market accounts
    let bet_state_account = Bet::from_account_info(bet_state_account_info)?;
    let betting_market_account = BettingMarket::from_account_info(betting_market_account_info)?;

    // check it is correct betting market account
    if bet_state_account.betting_market != *betting_market_account_info.key {
        msg!("Incorrect betting market account");
        return Err(BetError::InvalidAccounts.into());
    }

    // check it is correct escrow account
    if bet_state_account.bet_escrow_account != *bet_escrow_account_info.key {
        msg!("Incorrect escrow account");
        return Err(BetError::InvalidAccounts.into());
    }

    // check bet hasn't been cancelled
    if bet_state_account.cancelled == true {
        return Err(BetError::BetCancelled.into());
    }

    // check it is correct oracle account
    if *pyth_oracle_price_account_info.key != bet_state_account.pyth_oracle_price_account {
        msg!("Invalid oracle account provided.");
        return Err(BetError::InvalidAccounts.into());
    }
    // get the current price of the asset
    let pyth_price_data = pyth_oracle_price_account_info.try_borrow_data()?;
    let price_account: Price = *load_price( &pyth_price_data ).unwrap();
    let price: PriceConf = price_account.get_current_price().unwrap();

    // check current price is valid for bet to be accepted
    if price.price > bet_state_account.cancel_condition.above_price || price.price < bet_state_account.cancel_condition.below_price {
        msg!("Price moved beyond cancel condition prices.");
        return Err(BetError::BetNoLongerValid.into());
    }

    // check time isn't too late
    if clock.unix_timestamp > bet_state_account.cancel_condition.time || clock.unix_timestamp > bet_state_account.expiration_time {
        msg!("Time too late to accept bet.");
        return Err(BetError::BetNoLongerValid.into());
    }

    // calculate the odds given the current price and variable odds condition
    let bet_odds: i64;
    if let Some(variable_odds) = bet_state_account.variable_odds {
        let odds_change: i64;
        let price_change: i64;
        if bet_state_account.bet_price > bet_state_account.start_price {
            // price starts below bet price, so when price increases, odds decrease
            price_change = price.price - bet_state_account.start_price;
            odds_change = 0 - (price_change / variable_odds);
        } else {
            // price starts above bet price, so when price increases the odds increase
            price_change = price.price - bet_state_account.start_price;
            odds_change = price_change / variable_odds;
        }
        bet_odds = bet_state_account.odds + odds_change;
    } else {
        bet_odds = bet_state_account.odds; // no variable odds so bet odds are unchanged
    }

    // assert odds > 100
    if bet_odds < 100 {
        return Err(BetError::InvalidOdds.into());
    }

    // given the odds, calculate how much the acceptor must pay
    let acceptor_payment_amount: u64 = bet_size * ((bet_odds - 100) as u64) / 100;

    // send payment from both escrow account and acceptor payment account
    if betting_market_account.sol_payment {

        // check program is owner of the accepted bet escrow account
        if accepted_bet_escrow_account_info.key != program_id {
            msg!("Program is not owner of the bet escrow account");
            return Err(BetError::IncorrectOwner.into());
        }

        // add lamports from escrow account program owns to accepted escrow account
        **bet_escrow_account_info.lamports.borrow_mut() = bet_escrow_account_info.lamports().checked_sub(bet_size).ok_or(BetError::AmountUnderflow)?;
        **accepted_bet_escrow_account_info.lamports.borrow_mut() = accepted_bet_escrow_account_info.lamports().checked_add(bet_size).ok_or(BetError::AmountOverflow)?;

        // system program to transfer lamports from acceptor_payment_account_info
        let transfer_lamports_from_acceptor_ix = system_instruction::transfer(
            &acceptor_payment_account_info.key,
            &accepted_bet_escrow_account_info.key,
            acceptor_payment_amount
        );
        invoke(
            &transfer_lamports_from_acceptor_ix,
            &[
                system_program_account_info.clone(),
                acceptor_payment_account_info.clone(),
                accepted_bet_state_account_info.clone()
            ]
        )?;
    } else {
        // get the pda address and bump seed (derived from the bet_escrow_account_info Pubkey and prefix "yoyobet")
        let bet_escrow_account_seeds = &[
            PREFIX.as_bytes(),
            bet_escrow_account_info.key.as_ref(),
        ];
        let (bet_escrow_account_pda, bump_seed) = Pubkey::find_program_address(bet_escrow_account_seeds, program_id);

        // set transfer authority of the accepted bet escrow to PDA
        let transfer_authority_change_ix = spl_token::instruction::set_authority(
            token_program_account_info.key,
            accepted_bet_escrow_account_info.key,
            Some(&bet_escrow_account_pda),
            spl_token::instruction::AuthorityType::AccountOwner,
            acceptor_main_account_info.key,
            &[&acceptor_main_account_info.key],
        )?;
        msg!("Calling the token program to transfer ownership authority to PDA...");
        invoke(
            &transfer_authority_change_ix,
            &[
                accepted_bet_escrow_account_info.clone(),
                acceptor_main_account_info.clone(),
                token_program_account_info.clone(),
            ],
        )?;

        // need the bump seed for the signer seeds for invoke signed
        let bet_escrow_account_transfer_seeds = &[
            PREFIX.as_bytes(),
            bet_escrow_account_info.key.as_ref(),
            &[bump_seed]
        ];

        // transfer tokens from bet_escrow_account
        let transfer_tokens_from_escrow_ix = spl_token::instruction::transfer(
            token_program_account_info.key, 
            bet_escrow_account_info.key, 
            accepted_bet_escrow_account_info.key,
            &bet_escrow_account_pda, 
            &[&bet_escrow_account_pda], 
            bet_size
        )?;
        invoke_signed(
            &transfer_tokens_from_escrow_ix, 
            &[
                token_program_account_info.clone(),
                bet_escrow_account_info.clone(),
                accepted_bet_escrow_account_info.clone(),
                pda_account_info.clone()
            ],
            &[bet_escrow_account_transfer_seeds]
        )?;

        // transfer tokens from acceptor_payment_account_info
        let transfer_tokens_from_acceptor_ix = spl_token::instruction::transfer(
            &token_program_account_info.key, 
            &acceptor_payment_account_info.key,
            &accepted_bet_escrow_account_info.key, 
            &acceptor_main_account_info.key, 
            &[&acceptor_main_account_info.key], 
            acceptor_payment_amount
        )?;
        invoke(
            &transfer_tokens_from_acceptor_ix,
            &[
                token_program_account_info.clone(),
                acceptor_payment_account_info.clone(),
                accepted_bet_escrow_account_info.clone(),
                acceptor_main_account_info.clone()
            ]
        )?;
    }

    // write data to accepted bet state account
    let mut accepted_bet_state_account = AcceptedBet::from_account_info(&accepted_bet_state_account_info)?;
    accepted_bet_state_account.bet = *bet_state_account_info.key;
    accepted_bet_state_account.accepted_bet_escrow_account = *accepted_bet_escrow_account_info.key;
    accepted_bet_state_account.acceptor_main_account = *acceptor_main_account_info.key;
    accepted_bet_state_account.acceptor_payment_account = *acceptor_payment_account_info.key;
    accepted_bet_state_account.bet_size = bet_size;
    accepted_bet_state_account.odds = bet_odds;
    accepted_bet_state_account.finalized = false;

    // pack the tournament_state_account
    accepted_bet_state_account.serialize(&mut &mut accepted_bet_state_account_info.data.borrow_mut()[..])?;

    Ok(())
}

pub fn process_cancel_bet<'a>(
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let creator_main_account_info = next_account_info(account_info_iter)?;
    let creator_payment_account_info = next_account_info(account_info_iter)?;
    let bet_state_account_info = next_account_info(account_info_iter)?;
    let bet_escrow_account_info = next_account_info(account_info_iter)?;
    let betting_market_account_info = next_account_info(account_info_iter)?;
    let token_program_account_info = next_account_info(account_info_iter)?;
    spl_token::check_program_account(token_program_account_info.key)?;
    let system_program_account_info = next_account_info(account_info_iter)?;
    if check_id(system_program_account_info.key) == false {
        return Err(BetError::InvalidSystemProgram.into());
    }
    let pda_account_info = next_account_info(account_info_iter)?;

    // check creator main account is signer
    if !creator_main_account_info.is_signer {
        return Err(BetError::IncorrectSigner.into());
    }

    // unpack state account data
    let mut bet_state_account = Bet::from_account_info(bet_state_account_info)?;
    let betting_market_account = BettingMarket::from_account_info(betting_market_account_info)?;

    // check creator main account created the bet
    if bet_state_account.creator_main_account != *creator_main_account_info.key {
        msg!("Signer did not create the bet!");
        return Err(BetError::InvalidAccounts.into());
    }

    // check it is correct betting market account
    if bet_state_account.betting_market != *betting_market_account_info.key {
        msg!("Incorrect betting market account");
        return Err(BetError::InvalidAccounts.into());
    }

    // check it is correct escrow account
    if bet_state_account.bet_escrow_account != *bet_escrow_account_info.key {
        msg!("Incorrect escrow account");
        return Err(BetError::InvalidAccounts.into());
    }

    // send lamports / tokens from escrow account to creator payment account
    if betting_market_account.sol_payment == true {
        msg!("Calling system program to transfer tokens to bet creator");
        let transfer_lamports_from_escrow_ix = system_instruction::transfer(
            &bet_escrow_account_info.key,
            &creator_payment_account_info.key,
            bet_escrow_account_info.lamports()
        );
        invoke(
            &transfer_lamports_from_escrow_ix,
            &[
                system_program_account_info.clone(),
                bet_escrow_account_info.clone(),
                creator_payment_account_info.clone()
            ]
        )?;
    } else {
        // get pda address, bump seed and seeds
        let bet_escrow_account_seeds = &[
            PREFIX.as_bytes(),
            bet_escrow_account_info.key.as_ref(),
        ];
        let (bet_escrow_account_pda, bump_seed) = Pubkey::find_program_address(bet_escrow_account_seeds, program_id);
        let bet_escrow_transfer_seeds = &[
            PREFIX.as_bytes(),
            bet_escrow_account_info.key.as_ref(),
            &[bump_seed]
        ];

        // unpack token account to get amount in there
        let bet_escrow_account = TokenAccount::unpack_from_slice(&bet_escrow_account_info.data.borrow())?;

        msg!("Calling token program to transfer tokens to bet creator");
        let transfer_tokens_from_escrow_ix = spl_token::instruction::transfer(
            token_program_account_info.key, 
            bet_escrow_account_info.key, 
            creator_payment_account_info.key, 
            &bet_escrow_account_pda, 
            &[&bet_escrow_account_pda], 
            bet_escrow_account.amount
        )?;
        invoke_signed(
            &transfer_tokens_from_escrow_ix, 
            &[
                token_program_account_info.clone(),
                bet_escrow_account_info.clone(),
                creator_payment_account_info.clone(),
                pda_account_info.clone()
            ], 
            &[bet_escrow_transfer_seeds]
        )?;
    }

    // cancel the bet so noone in future can try to accept it
    bet_state_account.cancelled = true;

    // pack the bet_state_account
    bet_state_account.serialize(&mut &mut bet_state_account_info.data.borrow_mut()[..])?;

    Ok(())
}

pub fn process_finalize_bet<'a>(
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let finalizer_main_account_info = next_account_info(account_info_iter)?;
    let finalizer_payment_account_info = next_account_info(account_info_iter)?;
    let commission_fee_account_info = next_account_info(account_info_iter)?;
    let bet_state_account_info = next_account_info(account_info_iter)?;
    let accepted_bet_state_account_info = next_account_info(account_info_iter)?;
    let accepted_bet_escrow_account_info = next_account_info(account_info_iter)?;
    let creator_payment_account_info = next_account_info(account_info_iter)?;
    let acceptor_payment_account_info = next_account_info(account_info_iter)?;
    let betting_market_account_info = next_account_info(account_info_iter)?;
    let pyth_oracle_price_account_info = next_account_info(account_info_iter)?;
    let token_program_account_info = next_account_info(account_info_iter)?;
    spl_token::check_program_account(token_program_account_info.key)?;
    let system_program_account_info = next_account_info(account_info_iter)?;
    if check_id(system_program_account_info.key) == false {
        return Err(BetError::InvalidSystemProgram.into());
    }
    let pda_account_info = next_account_info(account_info_iter)?;
    let clock = &Clock::from_account_info(next_account_info(account_info_iter)?)?;

    if !finalizer_main_account_info.is_signer {
        return Err(BetError::IncorrectSigner.into());
    }

    // unpack the state accounts
    let bet_state_account = Bet::from_account_info(bet_state_account_info)?;
    let mut accepted_bet_state_account = AcceptedBet::from_account_info(accepted_bet_state_account_info)?;
    let betting_market_account = BettingMarket::from_account_info(betting_market_account_info)?;
    // check bet hasn't already been finalized
    if accepted_bet_state_account.finalized {
        msg!("Bet already finalized");
        return Err(BetError::BetFinalized.into());
    }
    // check it is correct betting market account
    if bet_state_account.betting_market != *betting_market_account_info.key {
        msg!("Wrong betting market account");
        return Err(BetError::InvalidAccounts.into());
    }
    // check it is correct pyth oracle account
    if bet_state_account.pyth_oracle_price_account != *pyth_oracle_price_account_info.key {
        msg!("Wrong pyth price account");
        return Err(BetError::InvalidAccounts.into());
    }
    // check it is correct commission fee account
    if betting_market_account.fee_commission_account != *commission_fee_account_info.key {
        msg!("Wrong commission fee account");
        return Err(BetError::InvalidAccounts.into());
    }
    // check it is correct creator account
    if bet_state_account.creator_payment_account != *creator_payment_account_info.key {
        msg!("Wrong bet creator payment account");
        return Err(BetError::InvalidAccounts.into());
    }
    // check it is correct acceptor payment account
    if accepted_bet_state_account.acceptor_payment_account != *acceptor_payment_account_info.key {
        msg!("Wrong bet acceptor payment account");
        return Err(BetError::InvalidAccounts.into());
    }
    // check it is correct escrow account
    if accepted_bet_state_account.accepted_bet_escrow_account != * accepted_bet_state_account_info.key {
        msg!("Wrong escrow account");
        return Err(BetError::InvalidAccounts.into());
    }

    // check time is after bet expiration time
    if clock.unix_timestamp < bet_state_account.expiration_time {
        msg!("Time is before bet expiration time");
        return Err(BetError::BeforeExpiryTime.into());
    }

    // get price from pyth oracle
    let pyth_price_data = pyth_oracle_price_account_info.try_borrow_data()?;
    let price_account: Price = *load_price( &pyth_price_data ).unwrap();
    let price: PriceConf = price_account.get_current_price().unwrap();

    // determine the bet winner
    let bet_winner_account_info: &AccountInfo;
    match bet_state_account.bet_direction {
        Direction::Above => {
            if price.price >= bet_state_account.bet_price {
                bet_winner_account_info = creator_payment_account_info;
            } else {
                bet_winner_account_info = acceptor_payment_account_info;
            }
        },
        Direction::Below => {
            if price.price <= bet_state_account.bet_price {
                bet_winner_account_info = creator_payment_account_info;
            } else {
                bet_winner_account_info = acceptor_payment_account_info;
            }
        } 
    }

    // calculate commission amount
    let commission_amount = accepted_bet_state_account.bet_size / 50;
    let finalizer_amount = commission_amount / 4;
    let winner_amount = accepted_bet_state_account.bet_size - commission_amount - finalizer_amount;

    // send payments to commission, winner and finalizer
    if betting_market_account.sol_payment {
        // transfer to commission account
        msg!("Calling system program to transfer lamports to commission account");
        let transfer_lamports_from_escrow_to_commission_ix = system_instruction::transfer(
            &accepted_bet_escrow_account_info.key,
            &commission_fee_account_info.key,
            commission_amount
        );
        invoke(
            &transfer_lamports_from_escrow_to_commission_ix,
            &[
                system_program_account_info.clone(),
                accepted_bet_escrow_account_info.clone(),
                commission_fee_account_info.clone()
            ]
        )?;

        // transfer to finalizer
        msg!("Calling system program to transfer lamports to finalizer account");
        let transfer_lamports_from_escrow_to_finalizer_ix = system_instruction::transfer(
            &accepted_bet_escrow_account_info.key,
            &finalizer_payment_account_info.key,
            finalizer_amount
        );
        invoke(
            &transfer_lamports_from_escrow_to_finalizer_ix,
            &[
                system_program_account_info.clone(),
                accepted_bet_escrow_account_info.clone(),
                finalizer_payment_account_info.clone()
            ]
        )?;

        // transfer to winner
        msg!("Calling system program to transfer lamports to finalizer account");
        let transfer_lamports_from_escrow_to_winner_ix = system_instruction::transfer(
            &accepted_bet_escrow_account_info.key,
            &bet_winner_account_info.key,
            winner_amount
        );
        invoke(
            &transfer_lamports_from_escrow_to_winner_ix,
            &[
                system_program_account_info.clone(),
                accepted_bet_escrow_account_info.clone(),
                bet_winner_account_info.clone()
            ]
        )?;
    } else {
        // get pda address, bump seed and seeds
        let bet_escrow_account_seeds = &[
            PREFIX.as_bytes(),
            bet_state_account.bet_escrow_account.as_ref(),
        ];
        let (bet_escrow_account_pda, bump_seed) = Pubkey::find_program_address(bet_escrow_account_seeds, program_id);
        let bet_escrow_transfer_seeds = &[
            PREFIX.as_bytes(),
            bet_state_account.bet_escrow_account.as_ref(),
            &[bump_seed]
        ];

        // transfer tokens to commission account
        msg!("Calling token program to transfer tokens to commission account");
        let transfer_tokens_from_escrow_to_commission_ix = spl_token::instruction::transfer(
            token_program_account_info.key, 
            accepted_bet_escrow_account_info.key, 
            commission_fee_account_info.key, 
            &bet_escrow_account_pda, 
            &[&bet_escrow_account_pda], 
            commission_amount
        )?;
        invoke_signed(
            &transfer_tokens_from_escrow_to_commission_ix, 
            &[
                token_program_account_info.clone(),
                accepted_bet_escrow_account_info.clone(),
                commission_fee_account_info.clone(),
                pda_account_info.clone()
            ], 
            &[bet_escrow_transfer_seeds]
        )?;

        // transfer tokens to winner payment account
        msg!("Calling token program to transfer tokens to commission account");
        let transfer_tokens_from_escrow_to_winner_ix = spl_token::instruction::transfer(
            token_program_account_info.key, 
            accepted_bet_escrow_account_info.key, 
            bet_winner_account_info.key, 
            &bet_escrow_account_pda, 
            &[&bet_escrow_account_pda], 
            winner_amount
        )?;
        invoke_signed(
            &transfer_tokens_from_escrow_to_winner_ix, 
            &[
                token_program_account_info.clone(),
                accepted_bet_escrow_account_info.clone(),
                bet_winner_account_info.clone(),
                pda_account_info.clone()
            ], 
            &[bet_escrow_transfer_seeds]
        )?;

        // transfer tokens to finalizer payment account
        msg!("Calling token program to transfer tokens to finalizer account");
        let transfer_tokens_from_escrow_to_finalizer_ix = spl_token::instruction::transfer(
            token_program_account_info.key, 
            accepted_bet_escrow_account_info.key, 
            finalizer_payment_account_info.key, 
            &bet_escrow_account_pda, 
            &[&bet_escrow_account_pda], 
            finalizer_amount
        )?;
        invoke_signed(
            &transfer_tokens_from_escrow_to_finalizer_ix, 
            &[
                token_program_account_info.clone(),
                accepted_bet_escrow_account_info.clone(),
                finalizer_payment_account_info.clone(),
                pda_account_info.clone()
            ], 
            &[bet_escrow_transfer_seeds]
        )?;
    }

    // update accepted bet state, set finalized to true
    accepted_bet_state_account.finalized = true;

    // pack state account
    accepted_bet_state_account.serialize(&mut &mut accepted_bet_state_account_info.data.borrow_mut()[..])?;

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

    Ok(())
}