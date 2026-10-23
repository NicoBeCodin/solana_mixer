
## Colosseum Eterbal Hackathon Finalist June 2026

# LeafPay 

These docs serve as a simple explainer as to how LeafPay works under the hood.

- On-chain Merkle Mountain Range architecture  
- Leaf parsing via Base64 memos  
- Transfering & withdrawing funds
- Client-side SNARK‐proof offloading  
- Roadmap: nullifier storage, multi-asset support, RPC endpoints  

---

## Merkle Tree Architecture


We maintain a **Merkle Mountain Range (MMR)** on‐chain, storing only *peaks* plus a 16‐leaf in‐flight buffer. Each batch is a 16‐leaf subtree (2⁴), whose root merges immediately into the MMR. By keeping only the current 16‐slot buffer **plus** peak array (`[depth, root]` pairs) on‐chain, all prove/deepen/roll‐up ops run in _O(log N)_ time without storing millions of leaves.  
A leaf is a hash of 3 values:




``` js
hash(amount|nullifier|assetId)
```

This allows for ZK proofs that leaves being deposited or funds being transfered correspond to actual amounts that are locked in the pool.

## Transfers & Withdrawals
Transfers work by proving that you know the preimage to one (or two) of the leaves of the pool, that the amount written on the leaf you're adding to the tree is equal to the amount on the leaves you're using and nullifying and that the asset match. Transfering funds to another user implies giving him the amount, nullifier and asset on your leaf.  

For withdrawal, the Groth16 proof now binds the recipient to the note's private nullifier. A relayer can submit the proof and pay the transaction fee without being able to change the payout address. Privacy still depends on the anonymity set and observable timing and amounts.

---

## Efficient Leaf Parsing with Memos

At each sub‐batch (8 leaves) and full‐batch (16 leaves), we emit a **Base64 memo** via the Solana Memo program. Payload format:

```text
batchNumber (8 bytes BE) ‖ leaf0 (32 bytes) ‖ … ‖ leafN (32 bytes)
```

Off‐chain indexers can call

```js
getSignaturesForAddress(LeavesIndexerPDA, …)
```

and replay memos to reconstruct deposited leaves. The small-tree indexer stores subtree roots so a client can avoid rebuilding the entire tree. Actual RPC cost depends on the tree position, pagination, and endpoint limits.

---
## Nullifier storage

Version 2 uses one root shard. A leaf shard holds up to 64 sorted nullifiers, then anyone can submit a separate split transaction to create two child shards. The split is routed by bits of `SHA-256(public_nullifier_hash)`, which avoids the fixed leading zero bits of BN254 field elements. The original nullifier hashes remain the values stored and checked for replay. Old pools have version 0 and cannot use the version 2 shard namespace. See [the measured cost report](research/leafpay_v2_sharding.md).


---

## Client‐Side Offloading


The browser generates SNARK proofs, packs public inputs, and reconstructs Merkle paths from on-chain state and memos. The program verifies proofs, roots, recipients, nullifiers, and required memos. The optional devnet relayer signs transactions on a server; its private key is never bundled into the browser.

---

## Work in Progress & Next Steps
* **Multi‐asset support:** Open deposits to SPL tokens, LSTs and NFTs in the same pool. The leaf format allows this but current anchor compatibility issues have halted the development of this feature.
* **Inbox system** Allow a user depositing funds to add an encrypted message only decryptable by the recipient using chacha symetric encryption. This feature requires a wallet capable of trying multiple Chacha key generations which isn't supported currently by most Solana wallets.

 * **Make a DAO** As a anonymity tool, the end goal is to make this community-owned and allow for a community of passionates to contribute to the future of encrypted DeFi.
