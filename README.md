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

## License
This software is provided under the MIT License.
However, commercial use of this software is strictly prohibited without explicit written permission from the author

---
