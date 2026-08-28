---
"@gpuix/native": patch
"@gpuix/react": patch
---

Make the masked backdrop blur radius linear in the mask value. The old
mapping split the mask range into three equal parts and crossfaded the
blur levels with a smoothstep per part. A third of a gradient mask then
went to blurs too small to see, and the visible change bunched into a
narrow band. The new mapping picks the two levels around `mask * radius`
and mixes them by kernel variance, so the blur width follows the
gradient across its whole ramp.
