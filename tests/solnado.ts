import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Solnado } from "../target/types/solnado";
import { expect } from "chai";

describe("solnado", () => {
  // Configure the client to use the local cluster.
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.Solnado as Program<Solnado>;

  let poolKeypair: anchor.web3.Keypair;

  // A convenience function to get the balance of an account.
  async function getBalance(pubkey: anchor.web3.PublicKey) {
    return await provider.connection.getBalance(pubkey);
  }

  before(async () => {
    // Generate a fresh keypair for the Pool account.
    // We'll create it in the InitializePool instruction.
    poolKeypair = anchor.web3.Keypair.generate();
  });
  console.log(`poolKeypair publick key: ${poolKeypair.publicKey}`)
  it("Initialize Pool", async () => {
    // We'll fund the poolKeypair so it can be created on-chain.
    const signature = await provider.connection.requestAirdrop(
      provider.wallet.publicKey,
      2 * anchor.web3.LAMPORTS_PER_SOL
    );
    await provider.connection.confirmTransaction(signature);

    // We call `initializePool`, providing our poolKeypair as the `pool`.
    await program.methods
      .initializePool()
      .accounts({
        pool: poolKeypair.publicKey,
        authority: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .signers([poolKeypair])
      .preInstructions([
        // Create the account using system program, matching the space in our Pool struct
        anchor.web3.SystemProgram.createAccount({
          fromPubkey: provider.wallet.publicKey,
          newAccountPubkey: poolKeypair.publicKey,
          space: 8 +  // Anchor account discriminator
            32 +      // merkle_root
            8 * 32 +  // leaves
            1 +       // next_index
            512,      // used_nullifiers (buffer)
          lamports: await provider.connection.getMinimumBalanceForRentExemption(
            8 + 32 + 8 * 32 + 1 + 512
          ),
          programId: program.programId,
        }),
      ])
      .rpc();

    const poolAccount = await program.account.pool.fetch(poolKeypair.publicKey);
    expect(poolAccount.merkleRoot).to.deep.equal(new Array(32).fill(0));
    expect(poolAccount.nextIndex).to.equal(0);
    console.log("Pool initialized successfully!");
  });

  it("Deposit 0.1 SOL (fixed) with a leaf", async () => {
    // Let's craft a "fake" leaf. In a real scenario, it's H(nullifier||secret).
    // We'll just pick random 32 bytes for demonstration.
    const fakeLeaf = new Uint8Array(32);
    // window.crypto?.getRandomValues?.(fakeLeaf); // If running in a browser-like environment
    // or use Node.js random fill:
    for (let i = 0; i < 32; i++) {
      fakeLeaf[i] = Math.floor(Math.random() * 256);
    }

    // Check initial balances:
    const userBalanceBefore = await getBalance(provider.wallet.publicKey);
    const poolBalanceBefore = await getBalance(poolKeypair.publicKey);
    console.log("User balance before deposit:", userBalanceBefore);
    console.log("Pool balance before deposit:", poolBalanceBefore);

    // Call `deposit`.
    await program.methods
      .deposit(Array.from(fakeLeaf)) // anchor treats typed arrays as normal arrays
      .accounts({
        pool: poolKeypair.publicKey,
        depositor: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // Check balances again
    const userBalanceAfter = await getBalance(provider.wallet.publicKey);
    const poolBalanceAfter = await getBalance(poolKeypair.publicKey);
    console.log("User balance after deposit:", userBalanceAfter);
    console.log("Pool balance after deposit:", poolBalanceAfter);

    // The user should have 0.1 SOL less
    expect(userBalanceBefore - userBalanceAfter).to.be.greaterThanOrEqual(
      0.1 * anchor.web3.LAMPORTS_PER_SOL
    );

    // The pool should have 0.1 SOL more
    expect(poolBalanceAfter - poolBalanceBefore).to.be.greaterThanOrEqual(
      0.1 * anchor.web3.LAMPORTS_PER_SOL
    );

    // Check that the leaf got inserted
    const poolAccount = await program.account.pool.fetch(poolKeypair.publicKey);
    expect(poolAccount.nextIndex).to.equal(1);
    console.log("Leaf inserted at index 0. Merkle root updated to:", poolAccount.merkleRoot);
  });

  it("Withdraw 0.1 SOL with a 'fake' proof", async () => {
    // We'll use a random nullifier, but in reality, this must match the deposit's nullifier
    // used to compute the leaf. For demonstration, we just pick random bytes.
    const fakeNullifier = new Uint8Array(32);
    for (let i = 0; i < 32; i++) {
      fakeNullifier[i] = Math.floor(Math.random() * 256);
    }

    // "zk_proof" is also just random in this test because our code has a placeholder verifier
    const fakeProof = new Uint8Array(64);
    for (let i = 0; i < 64; i++) {
      fakeProof[i] = Math.floor(Math.random() * 256);
    }

    const userBalanceBefore = await getBalance(provider.wallet.publicKey);
    const poolBalanceBefore = await getBalance(poolKeypair.publicKey);

    // Call `withdraw`
    await program.methods
      .withdraw(Array.from(fakeProof), Array.from(fakeNullifier))
      .accounts({
        pool: poolKeypair.publicKey,
        recipient: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const userBalanceAfter = await getBalance(provider.wallet.publicKey);
    const poolBalanceAfter = await getBalance(poolKeypair.publicKey);

    console.log("User balance after withdraw:", userBalanceAfter);
    console.log("Pool balance after withdraw:", poolBalanceAfter);

    // Expect user to get at least 0.1 SOL more
    expect(userBalanceAfter - userBalanceBefore).to.be.gte(
      0.1 * anchor.web3.LAMPORTS_PER_SOL - 10_000 // subtract some margin for fees
    );

    // The pool should have 0.1 SOL less
    expect(poolBalanceBefore - poolBalanceAfter).to.be.gte(
      0.1 * anchor.web3.LAMPORTS_PER_SOL - 10_000
    );

    // Verify that our fake nullifier was recorded in the used_nullifiers
    const poolAccount = await program.account.pool.fetch(poolKeypair.publicKey);
    // Convert to Buffer or array to compare
    const usedNullifiers = poolAccount.usedNullifiers.map((nf: number[]) => new Uint8Array(nf));
    expect(usedNullifiers.some((u) => u.toString() === fakeNullifier.toString())).to.be.true;

    console.log("Withdrawal succeeded! Nullifier was marked used.");
  });
  
});
