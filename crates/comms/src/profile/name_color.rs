use bevy::prelude::*;
use ethers_core::types::Address;

/// Hand-curated palette mirrored from `bevy-ui-scene`'s
/// `UserNameColors.json`. Keep this in lockstep with the scene so the marker
/// background matches the in-world nametag tint.
const PALETTE: [Srgba; 23] = [
    Srgba::new(0.671_385_05, 0.387_148_47, 0.943_396_2, 1.0),
    Srgba::new(0.832_455_7, 0.627_358_5, 1.0, 1.0),
    Srgba::new(0.871_691_4, 0.382_075_5, 1.0, 1.0),
    Srgba::new(1.0, 0.202_830_2, 0.978_383_7, 1.0),
    Srgba::new(1.0, 0.353_773_6, 0.923_547_45, 1.0),
    Srgba::new(1.0, 0.523_584_9, 0.796_823_14, 1.0),
    Srgba::new(1.0, 0.701_960_8, 0.943_320_4, 1.0),
    Srgba::new(1.0, 0.287_735_82, 0.309_539_65, 1.0),
    Srgba::new(1.0, 0.429_245_3, 0.467_913_36, 1.0),
    Srgba::new(1.0, 0.636_792_4, 0.666_241_65, 1.0),
    Srgba::new(1.0, 0.505_318_5, 0.080_188_69, 1.0),
    Srgba::new(1.0, 0.657_052_46, 0.0, 1.0),
    Srgba::new(1.0, 0.854_872_8, 0.0, 1.0),
    Srgba::new(1.0, 0.943_192_8, 0.608_490_6, 1.0),
    Srgba::new(0.515_649_26, 0.867_924_5, 0.0, 1.0),
    Srgba::new(0.619_413_7, 0.960_784_3, 0.121_568_605, 1.0),
    Srgba::new(0.858_401, 1.0, 0.561_320_8, 1.0),
    Srgba::new(0.0, 1.0, 0.728_798_4, 1.0),
    Srgba::new(0.533_018_8, 1.0, 0.935_397_8, 1.0),
    Srgba::new(0.607_843_16, 0.839_133_9, 1.0, 1.0),
    Srgba::new(0.607_843_16, 0.652_744_6, 1.0, 1.0),
    Srgba::new(0.485_849_08, 0.705_716_6, 1.0, 1.0),
    Srgba::new(0.278_301_9, 0.782_075_7, 1.0, 1.0),
];

/// Fallback for unclaimed-name users — `bevy-ui-scene` shows a flat grey for
/// this case rather than a hashed palette colour. We don't currently know
/// claimed-name status here, so the caller decides whether to use this.
pub const UNCLAIMED_NAME_COLOR: Color = Color::srgb(0.6, 0.6, 0.6);

/// 64-bit FNV-1a over the UTF-8 bytes of the lowercase hex address. Matches
/// `bevy-ui-scene`'s `simpleHash` exactly so palette indices line up with the
/// nametag scene.
fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 2166136261;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(16777619);
    }
    hash
}

pub fn name_color(address: Address) -> Color {
    let s = format!("{address:#x}");
    let idx = (fnv1a_64(s.as_bytes()) % PALETTE.len() as u64) as usize;
    Color::Srgba(PALETTE[idx])
}
