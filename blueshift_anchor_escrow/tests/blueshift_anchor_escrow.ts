import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { BlueshiftAnchorEscrow } from "../target/types/blueshift_anchor_escrow";
import {
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  getAssociatedTokenAddressSync,
  createMint,
  createAccount,
  mintTo,
  getAccount,
  getMint,
} from "@solana/spl-token";
import { PublicKey, Keypair, SystemProgram } from "@solana/web3.js";
import { expect } from "chai";

describe("blueshift_anchor_escrow", () => {
  // Configure the client to use the local cluster.
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.blueshiftAnchorEscrow as Program<BlueshiftAnchorEscrow>;

  // Test accounts
  let maker: Keypair;
  let taker: Keypair;
  let mintA: PublicKey;
  let mintB: PublicKey;
  let makerAtaA: PublicKey;
  let makerAtaB: PublicKey;
  let takerAtaA: PublicKey;
  let takerAtaB: PublicKey;
  let escrow: PublicKey;
  let vault: PublicKey;

  const seed = new anchor.BN(12345);
  const depositAmount = new anchor.BN(1000 * 10 ** 6); // 1000 tokens with 6 decimals
  const receiveAmount = new anchor.BN(500 * 10 ** 6); // 500 tokens with 6 decimals

  before(async () => {
    // Create keypairs for maker and taker
    maker = Keypair.generate();
    taker = Keypair.generate();

    // Airdrop SOL to maker and taker
    const airdropMaker = await provider.connection.requestAirdrop(
      maker.publicKey,
      2 * anchor.web3.LAMPORTS_PER_SOL
    );
    await provider.connection.confirmTransaction(airdropMaker);

    const airdropTaker = await provider.connection.requestAirdrop(
      taker.publicKey,
      2 * anchor.web3.LAMPORTS_PER_SOL
    );
    await provider.connection.confirmTransaction(airdropTaker);

    // Create mint A and mint B
    mintA = await createMint(
      provider.connection,
      maker,
      maker.publicKey,
      null,
      6
    );

    mintB = await createMint(
      provider.connection,
      taker,
      taker.publicKey,
      null,
      6
    );

    // Create associated token accounts
    makerAtaA = getAssociatedTokenAddressSync(mintA, maker.publicKey);
    makerAtaB = getAssociatedTokenAddressSync(mintB, maker.publicKey);
    takerAtaA = getAssociatedTokenAddressSync(mintA, taker.publicKey);
    takerAtaB = getAssociatedTokenAddressSync(mintB, taker.publicKey);

    // Create ATAs if they don't exist
    try {
      await getAccount(provider.connection, makerAtaA);
    } catch {
      await createAccount(provider.connection, maker, mintA, maker.publicKey);
    }

    try {
      await getAccount(provider.connection, makerAtaB);
    } catch {
      await createAccount(provider.connection, maker, mintB, maker.publicKey);
    }

    try {
      await getAccount(provider.connection, takerAtaA);
    } catch {
      await createAccount(provider.connection, taker, mintA, taker.publicKey);
    }

    try {
      await getAccount(provider.connection, takerAtaB);
    } catch {
      await createAccount(provider.connection, taker, mintB, taker.publicKey);
    }

    // Mint tokens to maker (Token A) and taker (Token B)
    await mintTo(
      provider.connection,
      maker,
      mintA,
      makerAtaA,
      maker,
      depositAmount.toNumber()
    );

    await mintTo(
      provider.connection,
      taker,
      mintB,
      takerAtaB,
      taker,
      receiveAmount.toNumber() * 2 // Give taker enough tokens
    );

    // Derive escrow PDA
    const [escrowPda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("escrow"),
        maker.publicKey.toBuffer(),
        seed.toArrayLike(Buffer, "le", 8),
      ],
      program.programId
    );
    escrow = escrowPda;

    // Derive vault PDA (associated token account for escrow)
    vault = getAssociatedTokenAddressSync(mintA, escrow, true);
  });

  it("Make: Creates an escrow and deposits tokens", async () => {
    const makerAtaABefore = await getAccount(provider.connection, makerAtaA);
    const makerBalanceBefore = Number(makerAtaABefore.amount);

    const tx = await program.methods
      .make(seed, receiveAmount, depositAmount)
      .accounts({
        maker: maker.publicKey,
        escrow: escrow,
        mintA: mintA,
        mintB: mintB,
        makerAtaA: makerAtaA,
        vault: vault,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([maker])
      .rpc();

    console.log("Make transaction signature:", tx);

    // Verify tokens were transferred from maker to vault
    const makerAtaAAfter = await getAccount(provider.connection, makerAtaA);
    const makerBalanceAfter = Number(makerAtaAAfter.amount);
    const vaultAccount = await getAccount(provider.connection, vault);

    expect(makerBalanceBefore - makerBalanceAfter).to.equal(depositAmount.toNumber());
    expect(Number(vaultAccount.amount)).to.equal(depositAmount.toNumber());

    // Verify escrow account was created
    const escrowAccount = await program.account.escrow.fetch(escrow);
    expect(escrowAccount.maker.toString()).to.equal(maker.publicKey.toString());
    expect(escrowAccount.mintA.toString()).to.equal(mintA.toString());
    expect(escrowAccount.mintB.toString()).to.equal(mintB.toString());
    expect(escrowAccount.receive.toString()).to.equal(receiveAmount.toString());
    expect(escrowAccount.seed.toString()).to.equal(seed.toString());
  });

  it("Take: Completes the escrow exchange", async () => {
    const takerAtaBBefore = await getAccount(provider.connection, takerAtaB);
    const makerAtaBBefore = await getAccount(provider.connection, makerAtaB);
    const takerBalanceBBefore = Number(takerAtaBBefore.amount);
    const makerBalanceBBefore = Number(makerAtaBBefore.amount || 0);

    const tx = await program.methods
      .take()
      .accounts({
        taker: taker.publicKey,
        maker: maker.publicKey,
        escrow: escrow,
        mintA: mintA,
        mintB: mintB,
        vault: vault,
        takerAtaA: takerAtaA,
        takerAtaB: takerAtaB,
        makerAtaB: makerAtaB,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([taker])
      .rpc();

    console.log("Take transaction signature:", tx);

    // Verify Token B was transferred from taker to maker
    const takerAtaBAfter = await getAccount(provider.connection, takerAtaB);
    const makerAtaBAfter = await getAccount(provider.connection, makerAtaB);
    const takerBalanceBAfter = Number(takerAtaBAfter.amount);
    const makerBalanceBAfter = Number(makerAtaBAfter.amount);

    expect(takerBalanceBBefore - takerBalanceBAfter).to.equal(receiveAmount.toNumber());
    expect(makerBalanceBAfter - makerBalanceBBefore).to.equal(receiveAmount.toNumber());

    // Verify Token A was transferred from vault to taker
    const takerAtaAAfter = await getAccount(provider.connection, takerAtaA);
    expect(Number(takerAtaAAfter.amount)).to.equal(depositAmount.toNumber());

    // Verify vault was closed (should throw error)
    let vaultClosed = false;
    try {
      await getAccount(provider.connection, vault);
    } catch (err: any) {
      vaultClosed = true;
      // Account should not exist anymore
    }
    expect(vaultClosed).to.equal(true);

    // Verify escrow was closed (should throw error)
    let escrowClosed = false;
    try {
      await program.account.escrow.fetch(escrow);
    } catch (err: any) {
      escrowClosed = true;
      // Account should not exist anymore
    }
    expect(escrowClosed).to.equal(true);
  });

  it("Refund: Allows maker to cancel escrow and get tokens back", async () => {
    // Mint more Token A to maker for the refund test
    await mintTo(
      provider.connection,
      maker,
      mintA,
      makerAtaA,
      maker,
      depositAmount.toNumber()
    );

    // First, create a new escrow for refund test
    const refundSeed = new anchor.BN(67890);
    const [refundEscrow] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("escrow"),
        maker.publicKey.toBuffer(),
        refundSeed.toArrayLike(Buffer, "le", 8),
      ],
      program.programId
    );
    const refundVault = getAssociatedTokenAddressSync(mintA, refundEscrow, true);

    // Make a new escrow
    await program.methods
      .make(refundSeed, receiveAmount, depositAmount)
      .accounts({
        maker: maker.publicKey,
        escrow: refundEscrow,
        mintA: mintA,
        mintB: mintB,
        makerAtaA: makerAtaA,
        vault: refundVault,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([maker])
      .rpc();

    // Get balance after making escrow (before refund)
    const makerAtaABeforeRefund = await getAccount(provider.connection, makerAtaA);
    const makerBalanceBeforeRefund = Number(makerAtaABeforeRefund.amount);

    // Verify vault has tokens
    const vaultBeforeRefund = await getAccount(provider.connection, refundVault);
    expect(Number(vaultBeforeRefund.amount)).to.equal(depositAmount.toNumber());

    const refundTx = await program.methods
      .refund()
      .accounts({
        maker: maker.publicKey,
        escrow: refundEscrow,
        mintA: mintA,
        vault: refundVault,
        makerAtaA: makerAtaA,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([maker])
      .rpc();

    console.log("Refund transaction signature:", refundTx);

    // Verify tokens were refunded to maker
    const makerAtaAAfterRefund = await getAccount(provider.connection, makerAtaA);
    const makerBalanceAfterRefund = Number(makerAtaAAfterRefund.amount);
    expect(makerBalanceAfterRefund - makerBalanceBeforeRefund).to.equal(depositAmount.toNumber());

    // Verify vault was closed
    let refundVaultClosed = false;
    try {
      await getAccount(provider.connection, refundVault);
    } catch (err: any) {
      refundVaultClosed = true;
      // Account should not exist anymore
    }
    expect(refundVaultClosed).to.equal(true);

    // Verify escrow was closed
    let refundEscrowClosed = false;
    try {
      await program.account.escrow.fetch(refundEscrow);
    } catch (err: any) {
      refundEscrowClosed = true;
      // Account should not exist anymore
    }
    expect(refundEscrowClosed).to.equal(true);
  });
});
