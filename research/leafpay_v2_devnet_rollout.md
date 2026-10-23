# LeafPay v2 devnet rollout

Deployed 2026-10-23 to the existing devnet program
`2xJgeatVVK3u3SNf4pyXuLuc2UrzEQBprPds2qfJSuEt`.

| Item | Address or transaction |
| --- | --- |
| Program upgrade | `4KVF1XQSv6r93kBXx3GEFEmujLFPG2jvjTd1kpdwG4YwJYJj1PxuUcw9M3bPPHhSFVrFfhyhZE1SkERLSPEEzkKD` |
| New `leafpayv2` pool | `2XPpJvgueMjX5z1w1uiag7ut2FHxDRWM24wiSXBJFdm3` |
| Pool initialization | `2nLdhU1q48oCaP4hvAny5vzRQ1mRJYE7wY8XLejXijPZQY8dNxujfDg8ewdJ6K2fhgChu2YP9Hv232U4WEmYG6bS` |
| Root nullifier shard | `71UBuNBjzJgQ7DtKeExoAX2ZwQ3ZAkJq5N6T9vKhf8ki` |
| Shard initialization | `46MhYxrQ54Ces4sx6VH36d8dhV3dYmn8k7sChzQarV3vY5yd56kZhutChH4EnbuD3572ZPjJJauqARGUHyQkBXi` |
| Relayer public key | `HVmxYErZauF4GGACJfCq5Z5Cb1iGP3j8UiWaBJc6NgUC` |

The deployed ELF starts with the same bytes as the locally built
`target/deploy/solnado.so` (SHA-256
`03c1d441836080264a387afc5070671412665998c669d78e73c1269be311e1a9`).
The program account contains trailing padding, so a SHA-256 of the full account
dump is different. The previous program dump is stored locally under
`deploy-backups/`, outside Git.

The old `defaultpool` was created by the v1 program. Its notes are incompatible
with v2 and cannot be spent through the new interface. The separate
`leafpayv2` pool prevents old spent notes from being replayed into the new
nullifier namespace. This is a devnet experiment; it is not a migration of old
notes.

The proving keys were generated from a local phase-2 contribution, and neither
the circuits nor the program have been independently audited. Do not use this
deployment for real funds.
