//! Make instruction: maker creates escrow, deposits token A into vault.

use core::mem::size_of;
use pinocchio::{
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    sysvars::{rent::Rent, Sysvar},
    ProgramResult,
};
use pinocchio_associated_token_account::instructions::Create;
use pinocchio_system::instructions::CreateAccount;
use pinocchio_token::instructions::TransferChecked;

use crate::instructions::helpers::find_escrow_address;
use crate::state::Escrow;

/// Make instruction data: seed (u64), receive (u64, amount of token B wanted), amount (u64, token A to deposit).
pub struct MakeInstructionData {
    pub seed: u64,
    pub receive: u64,
    pub amount: u64,
}

impl MakeInstructionData {
    pub const LEN: usize = size_of::<u64>() * 3;
}

impl<'a> core::convert::TryFrom<&'a [u8]> for MakeInstructionData {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        if data.len() < MakeInstructionData::LEN {
            return Err(ProgramError::InvalidInstructionData);
        }
        let seed = u64::from_le_bytes(data[0..8].try_into().unwrap());
        let receive = u64::from_le_bytes(data[8..16].try_into().unwrap());
        let amount = u64::from_le_bytes(data[16..24].try_into().unwrap());
        if receive == 0 || amount == 0 {
            return Err(ProgramError::InvalidInstructionData);
        }
        Ok(Self { seed, receive, amount })
    }
}

/// Make accounts: maker, escrow, mint_a, mint_b, maker_ata_a, vault, token_program, associated_token_program, system_program.
pub struct MakeAccounts<'a> {
    pub maker: &'a AccountInfo,
    pub escrow: &'a AccountInfo,
    pub mint_a: &'a AccountInfo,
    pub mint_b: &'a AccountInfo,
    pub maker_ata_a: &'a AccountInfo,
    pub vault: &'a AccountInfo,
    pub token_program: &'a AccountInfo,
    pub associated_token_program: &'a AccountInfo,
    pub system_program: &'a AccountInfo,
}

impl<'a> core::convert::TryFrom<&'a [AccountInfo]> for MakeAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [maker, escrow, mint_a, mint_b, maker_ata_a, vault, token_program, associated_token_program, system_program] =
            accounts else {
                return Err(ProgramError::NotEnoughAccountKeys);
            };

        if !maker.is_signer() {
            return Err(ProgramError::InvalidAccountOwner);
        }
        if token_program.key() != &pinocchio_token::ID {
            return Err(ProgramError::IncorrectProgramId);
        }
        if associated_token_program.key() != &pinocchio_associated_token_account::ID {
            return Err(ProgramError::IncorrectProgramId);
        }
        if system_program.key() != &pinocchio_system::ID {
            return Err(ProgramError::IncorrectProgramId);
        }

        Ok(Self {
            maker,
            escrow,
            mint_a,
            mint_b,
            maker_ata_a,
            vault,
            token_program,
            associated_token_program,
            system_program,
        })
    }
}

pub struct Make<'a> {
    pub accounts: MakeAccounts<'a>,
    pub data: MakeInstructionData,
}

impl<'a> core::convert::TryFrom<(&'a [u8], &'a [AccountInfo])> for Make<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        let accounts = MakeAccounts::try_from(accounts)?;
        let data = MakeInstructionData::try_from(data)?;

        let (escrow_key, _bump) = find_escrow_address(accounts.maker.key(), data.seed, &crate::ID);
        if accounts.escrow.key() != &escrow_key {
            return Err(ProgramError::InvalidAccountOwner);
        }

        Ok(Self { accounts, data })
    }
}

impl<'a> Make<'a> {
    pub fn process(&mut self) -> ProgramResult {
        let rent = Rent::get()?;
        let lamports = rent.minimum_balance(Escrow::LEN);

        let (_, bump) = find_escrow_address(self.accounts.maker.key(), self.data.seed, &crate::ID);
        let bump_binding = [bump];
        let seed_bytes = self.data.seed.to_le_bytes();
        let seeds = [
            Seed::from(b"escrow"),
            Seed::from(self.accounts.maker.key().as_ref()),
            Seed::from(seed_bytes.as_ref()),
            Seed::from(bump_binding.as_ref()),
        ];
        let signers = [Signer::from(&seeds)];

        CreateAccount {
            from: self.accounts.maker,
            to: self.accounts.escrow,
            lamports,
            space: Escrow::LEN as u64,
            owner: &crate::ID,
        }
        .invoke_signed(&signers)?;

        Create {
            funding_account: self.accounts.maker,
            account: self.accounts.vault,
            wallet: self.accounts.escrow,
            mint: self.accounts.mint_a,
            system_program: self.accounts.system_program,
            token_program: self.accounts.token_program,
        }
        .invoke()?;

        let mut escrow_data = self.accounts.escrow.try_borrow_mut_data()?;
        let escrow = Escrow::load_mut(&mut *escrow_data)?;
        escrow.set_inner(
            self.data.seed,
            *self.accounts.maker.key(),
            *self.accounts.mint_a.key(),
            *self.accounts.mint_b.key(),
            self.data.receive,
            [bump],
        );

        // SPL Mint decimals at offset 44
        const MINT_DECIMALS_OFFSET: usize = 44;
        let mint_data = self.accounts.mint_a.try_borrow_data()?;
        if mint_data.len() <= MINT_DECIMALS_OFFSET {
            return Err(ProgramError::InvalidAccountData);
        }
        let decimals = mint_data[MINT_DECIMALS_OFFSET];
        TransferChecked {
            from: self.accounts.maker_ata_a,
            mint: self.accounts.mint_a,
            to: self.accounts.vault,
            authority: self.accounts.maker,
            amount: self.data.amount,
            decimals,
        }
        .invoke()?;

        Ok(())
    }
}
