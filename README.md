# LeafPay on-chain program

LeafPay is an experimental zero-knowledge transfer pool on **Solana devnet**. This repository contains the Anchor program. The [web client and API](https://github.com/NicoBeCodin/solnado_website) contain the browser prover, circuit artifacts, relayer, and current integration scripts. The [live app](https://leafpay.xyz) uses the `leafpayv2` pool.

The project began during the June 2025 Colosseum hackathon and has since been updated with proof verification, recipient-bound relayed withdrawals, and sharded nullifier storage.

| Devnet item | Address |
| --- | --- |
| Program | [`2xJgeatVVK3u3SNf4pyXuLuc2UrzEQBprPds2qfJSuEt`](https://explorer.solana.com/address/2xJgeatVVK3u3SNf4pyXuLuc2UrzEQBprPds2qfJSuEt?cluster=devnet) |
| `leafpayv2` pool | [`2XPpJvgueMjX5z1w1uiag7ut2FHxDRWM24wiSXBJFdm3`](https://explorer.solana.com/address/2XPpJvgueMjX5z1w1uiag7ut2FHxDRWM24wiSXBJFdm3?cluster=devnet) |
| Root nullifier shard | [`71UBuNBjzJgQ7DtKeExoAX2ZwQ3ZAkJq5N6T9vKhf8ki`](https://explorer.solana.com/address/71UBuNBjzJgQ7DtKeExoAX2ZwQ3ZAkJq5N6T9vKhf8ki?cluster=devnet) |

The earlier `defaultpool` was created by the v1 program. Its notes cannot be spent through the v2 interface; `leafpayv2` is a separate pool, not a migration. Deployment details and transaction signatures are in [the devnet rollout record](research/leafpay_v2_devnet_rollout.md).

## How it works

1. **Deposit.** A client generates a private note and a Poseidon commitment from its amount, nullifier, and asset identifier. A Groth16 proof establishes that the public commitment and deposited amount agree. The program transfers SOL into the pool and appends the commitment to its Merkle state. The v2 pool requires at least **0.005 SOL** per deposit.
2. **Build a proof path.** The pool stores a 16-leaf buffer and the peaks of a Merkle mountain range instead of every historical leaf. When each group of eight leaves is completed, the transaction must include a checked Solana Memo containing that group's commitments. The leaves-indexer account makes those transactions discoverable through RPC. A subtree root is indexed every 4,096 full batches, or 65,536 leaves. Clients reconstruct paths from this public history; they still depend on an RPC provider to fetch it.
3. **Spend.** The client proves knowledge of a note in the tree without identifying its leaf. The program checks the Groth16 proof, root, amount rules, and nullifier before moving funds or adding replacement commitments. A relayed withdrawal binds the recipient address into the proof, so the relayer can pay the network fee without changing the payout address.
4. **Prevent replay.** LeafPay stores public nullifier hashes in sorted shard accounts. A SHA-256 digest of each hash determines its shard-tree route; the original nullifier hash remains the stored replay key. A leaf shard can hold up to 64 entries before a separate transaction splits it into two children. The pool charges **0.0003 SOL per spent nullifier** to fund shard growth.

The current deployed pool accepts SOL. The commitment format includes an asset identifier, but SPL token and NFT deposits are not supported by this deployment. On-chain activity still exposes amounts and timing; a proof alone does not guarantee anonymity.

### V2 instructions

| Instruction | Role |
| --- | --- |
| `initialize_variable_pool`, `initialize_shards` | Create a new pool and its root nullifier shard |
| `deposit_variable` | Verify a deposit proof, transfer SOL, and append one or two commitments |
| `combine_deposit_shard_single`, `combine_deposit_shard_double` | Spend one or two notes and add replacement commitments |
| `withdraw_variable_shard` | Spend a note and pay a recipient directly |
| `withdraw_on_behalf_shard` | Spend a note through a fee-paying relayer with a proof-bound recipient |
| `split_shard` | Divide a full leaf shard into two child accounts |

A relayed withdrawal also deducts **0.00005 SOL** from the note amount for the relayer, in addition to the 0.0003 SOL pool fee. The relayer pays the Solana network fee separately.

## Code map

| Path | Purpose |
| --- | --- |
| [`programs/solnado/src/lib.rs`](programs/solnado/src/lib.rs) | Pool initialization, deposits, instruction entry points, and Merkle updates |
| [`programs/solnado/src/shard.rs`](programs/solnado/src/shard.rs) | Shard routing, insertion, splitting, spending, and relayed withdrawal |
| [`programs/solnado/src/utils.rs`](programs/solnado/src/utils.rs) | Merkle, proof, memo, and public-input helpers |
| [`programs/solnado/src/state.rs`](programs/solnado/src/state.rs) | Pool and shard account layouts |
| [`programs/solnado/src/verifying_key.rs`](programs/solnado/src/verifying_key.rs) | Embedded Groth16 verification keys |

## Build and test locally

Use **Anchor 0.31.1** (as specified in [`Anchor.toml`](Anchor.toml)), the compatible Solana SBF toolchain, and Node.js for the integration scripts. `Anchor.toml` currently points its provider wallet at a machine-specific path; set it to your own keypair before using Anchor commands that need a signer. A build does not require the devnet pool or a relayer key.

```bash
anchor build
solana-test-validator --reset \
  --bpf-program 2xJgeatVVK3u3SNf4pyXuLuc2UrzEQBprPds2qfJSuEt target/deploy/solnado.so
```

The current executable v2 integration checks are in the [web client repository](https://github.com/NicoBeCodin/solnado_website). In another terminal, from that repository, run `npm ci` and then:

```bash
node scripts/local-shard-test.mjs
```

That script creates a fresh local pool, exercises a real deposit and withdrawal proof, and checks that a replay and an invalid proof fail. The split test is two-stage: `node scripts/local-split-test.mjs prepare` writes pool and root-shard fixtures; restart the validator with those fixtures using `--account`, then run `node scripts/local-split-test.mjs run` and `node scripts/local-shard-test.mjs --postsplit`. See the scripts for their expected validator state and fixture paths.

The `tests/solnado.ts` file in this repository is an obsolete, commented-out v1 example. It is not a v2 test suite, so `anchor test` should not be treated as proof that the current program works.

## Measurements and current limits

[Local validator measurements](research/leafpay_v2_sharding.md) recorded **136,382 CU** for a verified 0.01 SOL deposit, **194,127 CU** for a direct withdrawal into the root shard, **196,853 CU** for a direct withdrawal after a split, and **42,862 CU** for the split itself. At 64 spent nullifiers, the shard accounts held **67.4% less rent** than 64 separate minimum-size marker accounts. These are measurements of this implementation and a rent comparison, not a total-cost benchmark against another privacy protocol. Devnet and mainnet results may differ.

The program and circuits have not been independently audited. The v2 proving keys came from a local phase-2 contribution and are suitable for this devnet experiment, not a mainnet launch without an independent ceremony and security review. Notes live with the client; losing a note can make its funds unrecoverable. See the [web client README](https://github.com/NicoBeCodin/solnado_website#readme) for note backups and API usage.
