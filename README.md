# Solana Mixer - On-Chain Logic

In our original “fixed-amount” mixer, every deposit was exactly the same size and went straight into a single 256-leaf Merkle tree.  The user’s only secret was a nullifier and a fixed “note” value, and withdrawals simply proved membership of that one note.

With the new **Shielded Pool** design, we’ve generalized and improved that model:

1. **Variable-Amount Notes**  
   - **Before**: every leaf committed exactly 0.1 SOL (or whatever fixed size).  
   - **Now**: each leaf carries its own private 64-bit value.  Two leaves can be deposited together—as long as their *sum* matches a third note you supply—so you can aggregate arbitrary amounts without revealing the individual addends on-chain.

2. **“Combine” Proofs**  
   - **Before**: deposits were independent, you could only withdraw one fixed note.  
   - **Now**: you prove in zero-knowledge that  
     \[ val₁ + val₂ = val₃ \]  
     without revealing `val₁` or `val₂`.  On success, the two old notes are nullified (lock out double-spend via their nullifier-PDAs) and the new summed note is appended.

3. **Multi-Stage Batching (Sub-Batch Memos)**  
   - **Before**: every deposit tx included a full 16-leaf batch in a single Memo instruction (520 bytes).  
   - **Now**: to avoid Solana’s “transaction too large” limit, we split each 16-leaf batch into two 8-leaf windows.  Whenever you cross an 8-leaf boundary (e.g. going from 7→9 or 15→17 leaves), the CLI auto-attaches *only* that 8-leaf window as a 264 byte memo.  The on-chain code then enforces byte-for-byte consistency of exactly those 8 leaves.

4. **Deep-Padding Merkle Trees**  
   - **Before**: tree depth was fixed at 8 (256 leaves).  
   - **Now**: we keep a small “active” subtree (next power of two of current deposits) and then “deepen” it via successive default-leaf Poseidon hashes up to a larger target depth (e.g. 20) to form a fixed-depth tree without storing millions of zeros on-chain.

5. **Enhanced CLI & Anchor APIs**  
   - **Before**: simple `deposit()`, `withdraw()`.  
   - **Now**: new endpoints for  
     - `initialize_variable_pool`  
     - `deposit_variable` (sum-proof + sub-batch memo)  
     - `generate_combine_proof` / `send_combine_deposit_proof`  
     - `generate_withdrawal_proof` / `send_withdrawal_proof`  
   - All commands automate memo‐packing, proof generation, Merkle-proof routing, and high-compute budget injection.

---

This new shielded-pool architecture preserves the core privacy guarantees of the original mixer (no link between deposit and withdrawal) while unlocking **variable amounts**, **on-chain note-combining**, and **scalable** tree depths—without blowing past Solana’s transaction‐size or compute limits.  


---

# Fixed amount pool implmenetation

## Overview
Solana Mixer is a privacy-preserving transaction system implemented on Solana using Anchor and zk-SNARKs. It allows users to deposit fixed amounts of SOL into a shielded pool and later withdraw them anonymously by proving ownership of deposited funds via zero-knowledge proofs. 
To interact with this program, look at the solana_mixer_cli on my github page.

## Features
- **Zero-Knowledge Proofs**: Uses Groth16 zk-SNARKs for anonymous withdrawals.
- **Merkle Tree Commitments**: Deposits are stored in a Merkle tree to prove membership efficiently.
- **Low cost**: The cost of withdrawal is capped at 0.001 sol to create a PDA to store the nullifier hash.

1. **Deposit** fixed or _variable_ amounts of SOL into a shielded pool.  
2. **Combine** two existing “notes” _(leaves)_ whose private values sum to a new note, appending the result back into the pool.  
3. **Withdraw** anonymously by proving membership of a leaf (with its hidden amount) and spending it exactly once via nullifier-hash locks.


## Merkle Tree Implementation
- **Depth**: Is adjustable manually, currently at 8 for 256 leaves
- **Storage** : efficient storage with merkle mountain range, storing leaves permananently on the solana ledger with the client having to bring the correct leaves to have a valid transaction.
- **Padding**: Tree is padded with default leaves to have a fixed size tree but efficiently stored.
- **Default Leaf**: `DEFAULT_LEAF` ensures new trees are initialized correctly.
- **Merkle Root Computation**: Uses Poseidon hashing to generate intermediate and final root nodes. The `deepen` tree function is used to make the tree root deeper to match a certain size.

## Security Measures
- **Nullifier Check**: Prevents reuse of proofs by creating a pda with nullifier as seed, it is important to make unique one.
- **Merkle Root Validation**: Ensures withdrawals reference a valid state of the deposit pool.
- **Anchor Security Features**: Uses program-derived addresses (PDAs) to manage funds securely.

## Dependencies
- **Solana Poseidon**: Cryptographic hash function optimized for zk-SNARK circuits.
- **Groth16-Solana**: Verifier for zk-SNARK proofs.
- **Anchor**: Framework for Solana smart contracts.

## Next Steps
- Explore support for variable deposit amounts while maintaining anonymity guarantees.
- Make the tree parameters modular to allow users to create their custom pools.

## License
This software is provided under the MIT License.
However, commercial use of this software is strictly prohibited without explicit written permission from the author.321




