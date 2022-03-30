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
    system_instruction
};

use spl_token::state::Account as TokenAccount;

use crate::{
    instruction::FantasyCryptoInstruction,
    error::BetError,
    utils::PREFIX,
    utils::create_or_allocate_account_raw,
    utils::puffed_out_string,
    state::TournamentAccount,
};

// use std::convert::TryInto;
use borsh::{BorshSerialize, BorshDeserialize};

use pyth_client::{
    Price,
    PriceConf,
    load_price
};

pub fn process_instruction<'a>(
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
    input: &[u8],
) -> ProgramResult {
    let instruction = FantasyCryptoInstruction::try_from_slice(input)?;
    match instruction {
        FantasyCryptoInstruction::CreateTournament(args) => {
            msg!("Instruction: Create Tournament");
            process_create_tournament(
                program_id,
                accounts,
                args.entry_fee,
                args.commission_basis_points,
                args.max_num_players,
                args.product_account_list,
                args.duration,
                args.pay_with_sol,
                args.payment_token_mint,
            )
        },
    }
}

pub fn process_create_tournament<'a>(
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
    entry_fee: u64,
    commission_basis_points: u16,
    max_num_players: u8,
    product_account_list: Vec<Pubkey>,
    duration: u64,
    pay_with_sol: bool,
    payment_token_mint: Option<Pubkey>,
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let creator_account_info = next_account_info(account_info_iter)?;
    let tournament_state_account_info = next_account_info(account_info_iter)?;
    let prize_pool_account_info = next_account_info(account_info_iter)?;
    let commission_account_info = next_account_info(account_info_iter)?;
    let rent = &Rent::from_account_info(next_account_info(account_info_iter)?)?;
    let token_program_account = next_account_info(account_info_iter)?;
    spl_token::check_program_account(token_program_account.key)?;

    // check creator_account_info is the tx signer
    if !creator_account_info.is_signer {
        return Err(BetError::IncorrectSigner.into());
    }

    // check program is owner of the tournament_state_account_info
    if tournament_state_account_info.owner != program_id {
        return Err(BetError::IncorrectOwner.into());
    }

    // check tournament_state_account_info has enough lamports to be rent exempt
    if !rent.is_exempt(tournament_state_account_info.lamports(), tournament_state_account_info.data_len()) {
        return Err(BetError::NotRentExempt.into());
    }

    // check if it is a tournament with entry paid in SOL
    if pay_with_sol == true {
        // if yes, check that the program id is owner of the prize pool account
        if prize_pool_account_info.owner != program_id {
            return Err(BetError::IncorrectOwner.into());
        }
    } else if pay_with_sol == false {
        // if no, check prize pool and commission accounts are token accounts with mint = payment_token_mint
        if *prize_pool_account_info.owner != spl_token::ID || *commission_account_info.owner != spl_token::ID {
            return Err(BetError::IsNotTokenAccount.into());
        }
        // unpack the token account data
        let prize_pool_account = TokenAccount::unpack_from_slice(&prize_pool_account_info.data.borrow())?;
        let commission_account = TokenAccount::unpack_from_slice(&commission_account_info.data.borrow())?;
        // panics if pay with sol is false but we don't have a payment token mint
        if prize_pool_account.mint != payment_token_mint.unwrap() || commission_account.mint != payment_token_mint.unwrap() {
            return Err(BetError::InvalidMint.into());
        }

        // get the PDA account Pubkey (derived from the tournament_state_account_info Pubkey and prefix "FantasyCrypto")
        let prize_pool_account_seeds = &[
            PREFIX.as_bytes(),
            tournament_state_account_info.key.as_ref(),
        ];
        let (prize_pool_account_pda, _bump_seed) = Pubkey::find_program_address(prize_pool_account_seeds, program_id);

        
        // call token program to transfer ownership of prize pool account to PDA
        let transfer_authority_change_ix = spl_token::instruction::set_authority(
            token_program_account.key,
            prize_pool_account_info.key,
            Some(&prize_pool_account_pda),
            spl_token::instruction::AuthorityType::AccountOwner,
            creator_account_info.key,
            &[&creator_account_info.key],
        )?;
        msg!("Calling the token program to transfer ownership authority to PDA...");
        invoke(
            &transfer_authority_change_ix,
            &[
                prize_pool_account_info.clone(),
                creator_account_info.clone(),
                token_program_account.clone(),
            ],
        )?;
    }
    
    // unpack the tournament_state_account_info
    let mut tournament_state_account = TournamentAccount::from_account_info(&tournament_state_account_info)?;

    // check tournament state account hasn't already been initialized
    if tournament_state_account.is_initialized == true {
        return Err(BetError::InvalidTournamentAccount.into())
    }

    // write the data to state
    tournament_state_account.is_initialized = true;
    tournament_state_account.creator = *creator_account_info.key;
    tournament_state_account.started = false;
    tournament_state_account.finalized = false;
    tournament_state_account.entry_fee = entry_fee;
    tournament_state_account.commission_basis_points = commission_basis_points;
    tournament_state_account.max_num_players = max_num_players;
    tournament_state_account.product_account_list = product_account_list;
    tournament_state_account.start_timestamp = 0;
    tournament_state_account.duration = duration;
    tournament_state_account.pay_with_sol = pay_with_sol;
    tournament_state_account.payment_token_mint = payment_token_mint;
    tournament_state_account.prize_pool_account = *prize_pool_account_info.key;
    tournament_state_account.commission_account = *commission_account_info.key;
    let players: Vec<Pubkey> = Vec::new();
    tournament_state_account.players = players;
    let starting_prices: Vec<PriceConf> = Vec::new();
    tournament_state_account.starting_prices = starting_prices;

    // pack the tournament_state_account
    tournament_state_account.serialize(&mut &mut tournament_state_account_info.data.borrow_mut()[..])?;
   
    Ok(())
}