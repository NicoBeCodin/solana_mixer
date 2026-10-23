# LeafPay v2 nullifier-shard measurements

Measured on Solana test-validator 2.1.22 with the v2 SBF program, using the
scripts in the `solnado_website` repository. The program stores each original
Poseidon nullifier hash in one sorted leaf shard and uses SHA-256 of that hash
only to route it through the binary shard tree.

| Operation | Compute units |
| --- | ---: |
| Verified deposit of one 0.01 SOL note | 136,382 |
| Verified direct withdrawal into the root shard | 194,127 |
| Verified direct withdrawal after a split | 196,853 |
| Split a 64-nullifier shard into 30 and 34 entries | 42,862 |

The split test preloaded 64 sorted nullifiers into a root shard. It then used
the on-chain split instruction and inspected the parent and children. A valid
new deposit and withdrawal subsequently inserted into the selected child; a
second spend failed with `NullifierAlreadyUsed`. A changed recipient and an
invalid proof were rejected before the shard changed.

At that 64-nullifier point, the parent and two children held **19,752,480
lamports** of rent. The local validator quoted **946,560 lamports** for an
8-byte account, so 64 separate minimum-size marker accounts would hold
**60,579,840 lamports**. This makes the shard example **67.4% cheaper in
rent**. Shards use a 0.0003 SOL pool fee per spent nullifier to fund growth;
the split transaction's fee payer temporarily funds children and receives the
parent's released rent.

This is a storage comparison against a one-PDA-per-nullifier design, not a
full Privacy Cash transaction benchmark. Privacy Cash uses nullifier PDAs
according to its published source and audit; its proof system, instruction
path, and transaction composition differ. These measurements do not establish
that LeafPay has lower total transaction cost or compute use. Test-validator
rent and compute behavior may also differ from devnet and mainnet.

The v2 circuit setup was one local contribution from the existing phase-2
Powers of Tau file. Its proving and verification keys are suitable for this
devnet experiment, not for mainnet use without an independent multi-party
ceremony and security review. The on-chain program and client have not been
audited.
