use core::convert::TryFrom;
use pinocchio::{
    program_error::ProgramError,
    pubkey::{find_program_address, Pubkey},
    ProgramResult,
};
use pinocchio_secp256r1_instruction::Secp256r1Pubkey;
use pinocchio_system::instructions::Transfer;

use pinocchio::account_info::AccountInfo;

pub struct DepositAccounts<'a> {
    pub payer: &'a AccountInfo,
    pub vault: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for DepositAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [payer, vault, _] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        if !payer.is_signer() {
            return Err(ProgramError::InvalidAccountOwner);
        }

        if !vault.is_owned_by(&pinocchio_system::ID) {
            return Err(ProgramError::InvalidAccountOwner);
        }

        if vault.lamports().ne(&0) {
            return Err(ProgramError::InvalidAccountData);
        }

        Ok(Self { payer, vault })
    }
}

pub struct DepositInstructionData {
    pub pubkey: Secp256r1Pubkey,
    pub amount: u64,
}

impl<'a> TryFrom<&'a [u8]> for DepositInstructionData {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        if data.len() < 33 + 8 {
            return Err(ProgramError::InvalidInstructionData);
        }

        let (pubkey_bytes, amount_bytes) = data.split_at(33);

        let pubkey = Secp256r1Pubkey::try_from(pubkey_bytes)
            .map_err(|_| ProgramError::InvalidInstructionData)?;
        let amount = u64::from_le_bytes(
            amount_bytes
                .try_into()
                .map_err(|_| ProgramError::InvalidInstructionData)?,
        );

        Ok(Self { pubkey, amount })
    }
}

pub struct Deposit<'a> {
    pub accounts: DepositAccounts<'a>,
    pub instruction_data: DepositInstructionData,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for Deposit<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        let accounts = DepositAccounts::try_from(accounts)?;
        let instruction_data = DepositInstructionData::try_from(data)?;

        Ok(Self {
            accounts,
            instruction_data,
        })
    }
}

impl<'a> Deposit<'a> {
    pub fn process(&mut self) -> ProgramResult {
        let (vault_key, _) = find_program_address(
            &[
                b"vault",
                &self.instruction_data.pubkey[..1],
                &self.instruction_data.pubkey[1..33],
            ],
            &crate::ID,
        );
        if vault_key.ne(self.accounts.vault.key()) {
            return Err(ProgramError::InvalidAccountOwner);
        }

        Transfer {
            from: self.accounts.payer,
            to: self.accounts.vault,
            lamports: self.instruction_data.amount,
        }
        .invoke()?;

        Ok(())
    }
}
