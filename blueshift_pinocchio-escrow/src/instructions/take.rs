//! Take instruction: taker sends token B to maker, receives token A from vault; escrow and vault closed.

use pinocchio::{
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    ProgramResult,
};
use pinocchio_system::instructions::Transfer;
use pinocchio_token::instructions::{CloseAccount, TransferChecked};

use crate::state::Escrow;

// SPL Token Account amount at offset 64.
const TOKEN_ACCOUNT_AMOUNT_OFFSET: usize = 64;

/// Take accounts: taker, maker, escrow, mint_a, mint_b, vault, taker_ata_a, taker_ata_b, maker_ata_b, system_program, token_program, associated_token_program.
pub struct TakeAccounts<'a> {
    pub taker: &'a AccountInfo,
    pub maker: &'a AccountInfo,
    pub escrow: &'a AccountInfo,
    pub mint_a: &'a AccountInfo,
    pub mint_b: &'a AccountInfo,
    pub vault: &'a AccountInfo,
    pub taker_ata_a: &'a AccountInfo,
    pub taker_ata_b: &'a AccountInfo,
    pub maker_ata_b: &'a AccountInfo,
    pub system_program: &'a AccountInfo,
    pub token_program: &'a AccountInfo,
    pub associated_token_program: &'a AccountInfo,
}

impl<'a> core::convert::TryFrom<&'a [AccountInfo]> for TakeAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [
            taker, maker, escrow, mint_a, mint_b, vault,
            taker_ata_a, taker_ata_b, maker_ata_b,
            system_program, token_program, associated_token_program,
        ] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        if !taker.is_signer() {
            return Err(ProgramError::InvalidAccountOwner);
        }
        if !escrow.is_owned_by(&crate::ID) {
            return Err(ProgramError::InvalidAccountOwner);
        }
        let escrow_data = escrow.try_borrow_data()?;
        let escrow_state = Escrow::load(&*escrow_data)?;
        if escrow_state.maker != *maker.key() {
            return Err(ProgramError::InvalidAccountOwner);
        }
        if escrow_state.mint_a != *mint_a.key() || escrow_state.mint_b != *mint_b.key() {
            return Err(ProgramError::InvalidAccountOwner);
        }

        Ok(Self {
            taker,
            maker,
            escrow,
            mint_a,
            mint_b,
            vault,
            taker_ata_a,
            taker_ata_b,
            maker_ata_b,
            system_program,
            token_program,
            associated_token_program,
        })
    }
}

pub struct Take<'a> {
    pub accounts: TakeAccounts<'a>,
}

impl<'a> core::convert::TryFrom<&'a [AccountInfo]> for Take<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        Ok(Self {
            accounts: TakeAccounts::try_from(accounts)?,
        })
    }
}

impl<'a> Take<'a> {
    pub fn process(&mut self) -> ProgramResult {
        let escrow_data = self.accounts.escrow.try_borrow_data()?;
        let escrow = Escrow::load(&*escrow_data)?;
        let seed = escrow.seed;
        let bump = escrow.bump[0];
        let receive = escrow.receive;
        drop(escrow_data);

        let maker_key = self.accounts.maker.key();
        let seed_bytes = seed.to_le_bytes();
        let binding = [bump];
        let seeds = [
            Seed::from(b"escrow"),
            Seed::from(maker_key.as_ref()),
            Seed::from(seed_bytes.as_ref()),
            Seed::from(&binding),
        ];
        let signers = [Signer::from(&seeds)];

        let mint_b_data = self.accounts.mint_b.try_borrow_data()?;
        let decimals_b = if mint_b_data.len() > 44 { mint_b_data[44] } else { return Err(ProgramError::InvalidAccountData) };
        drop(mint_b_data);

        TransferChecked {
            from: self.accounts.taker_ata_b,
            mint: self.accounts.mint_b,
            to: self.accounts.maker_ata_b,
            authority: self.accounts.taker,
            amount: receive,
            decimals: decimals_b,
        }
        .invoke()?;

        let vault_data = self.accounts.vault.try_borrow_data()?;
        if vault_data.len() < TOKEN_ACCOUNT_AMOUNT_OFFSET + 8 {
            return Err(ProgramError::InvalidAccountData);
        }
        let vault_amount = u64::from_le_bytes(vault_data[TOKEN_ACCOUNT_AMOUNT_OFFSET..TOKEN_ACCOUNT_AMOUNT_OFFSET + 8].try_into().unwrap());
        drop(vault_data);

        let mint_a_data = self.accounts.mint_a.try_borrow_data()?;
        let decimals_a = if mint_a_data.len() > 44 { mint_a_data[44] } else { return Err(ProgramError::InvalidAccountData) };
        drop(mint_a_data);

        TransferChecked {
            from: self.accounts.vault,
            mint: self.accounts.mint_a,
            to: self.accounts.taker_ata_a,
            authority: self.accounts.escrow,
            amount: vault_amount,
            decimals: decimals_a,
        }
        .invoke_signed(&signers)?;

        CloseAccount {
            account: self.accounts.vault,
            destination: self.accounts.maker,
            authority: self.accounts.escrow,
        }
        .invoke_signed(&signers)?;

        let escrow_lamports = self.accounts.escrow.lamports();
        Transfer {
            from: self.accounts.escrow,
            to: self.accounts.maker,
            lamports: escrow_lamports,
        }
        .invoke_signed(&signers)?;

        Ok(())
    }
}
