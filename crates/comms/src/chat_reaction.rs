//! Chat message reactions: an emoji attached to one chat message by one wallet.
//!
//! On the wire (rfc4 `ChatReaction`) a reaction names its message by Unity's message id, built
//! from the sender and the rfc4 `Chat` timestamp, and its emoji by a tile index into Unity's
//! emoji atlas (`unity_emoji_atlas.txt`, the codepoints of each tile in atlas order). A remove is
//! the bitwise NOT of the index. We also send the emoji itself and a remove flag in bevy-private
//! fields, prefer the emoji on receive, and take either signal as a remove; everything above the
//! wire deals only in emoji strings.

use std::{collections::HashMap, sync::LazyLock};

use alloy_core::primitives::Address;
use bevy::prelude::*;
use common::util::AsH160;

/// Codepoints of each tile in Unity's emoji atlas (`emojis32.asset`), in tile order, as
/// lowercase hex joined by `_` with U+FE0F dropped.
const UNITY_EMOJI_ATLAS: &str = include_str!("chat_reaction/unity_emoji_atlas.txt");

/// What Unity shows for an emoji its atlas doesn't have: ⬜ (U+2B1C WHITE LARGE SQUARE).
const UNMAPPED: &str = "\u{2b1c}";

struct Atlas {
    /// The emoji each tile shows.
    emojis: Vec<String>,
    tiles: HashMap<&'static str, i32>,
    unmapped: i32,
}

static ATLAS: LazyLock<Atlas> = LazyLock::new(|| {
    let names = UNITY_EMOJI_ATLAS.lines().collect::<Vec<_>>();
    let tiles = names
        .iter()
        .enumerate()
        .map(|(i, name)| (*name, i as i32))
        .collect::<HashMap<_, _>>();
    let unmapped = tiles[atlas_name(UNMAPPED).as_str()];
    let emojis = names.iter().map(|name| tile_emoji(name)).collect();
    Atlas {
        emojis,
        tiles,
        unmapped,
    }
});

fn atlas_name(emoji: &str) -> String {
    emoji
        .chars()
        .filter(|c| *c != '\u{fe0f}')
        .map(|c| format!("{:04x}", c as u32))
        .collect::<Vec<_>>()
        .join("_")
}

/// The emoji a Unity atlas tile shows.
fn tile_emoji(name: &str) -> String {
    let mut emoji = name
        .split('_')
        .filter_map(|cp| char::from_u32(u32::from_str_radix(cp, 16).ok()?))
        .collect::<String>();
    // Unity's picker sends a keycap as its first codepoint, whose tile draws the keycap
    if matches!(
        emoji.as_str(),
        "#" | "*" | "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"
    ) {
        emoji.push_str("\u{fe0f}\u{20e3}");
    }
    canonical_emoji(&emoji).unwrap_or_else(|| UNMAPPED.to_owned())
}

/// `emoji` as one fully-qualified emoji, or `None` if it isn't one. A lone regional indicator
/// letter passes too: Unity's picker sends a flag as its first letter.
pub fn canonical_emoji(emoji: &str) -> Option<String> {
    if let Some(emoji) = emojis::get(emoji) {
        return Some(emoji.as_str().to_owned());
    }
    let mut chars = emoji.chars();
    match (chars.next(), chars.next()) {
        (Some(c @ '\u{1f1e6}'..='\u{1f1ff}'), None) => Some(c.to_string()),
        _ => None,
    }
}

/// The Unity atlas tile for an emoji: its own tile if the atlas has one, else ⬜.
pub fn emoji_to_atlas_index(emoji: &str) -> i32 {
    let atlas = &*ATLAS;
    atlas
        .tiles
        .get(atlas_name(emoji).as_str())
        .copied()
        .unwrap_or(atlas.unmapped)
}

/// The emoji a Unity atlas tile shows, or `None` for an index outside the atlas.
pub fn atlas_index_to_emoji(index: i32) -> Option<&'static str> {
    let atlas: &'static Atlas = &ATLAS;
    usize::try_from(index)
        .ok()
        .and_then(|i| atlas.emojis.get(i))
        .map(String::as_str)
}

/// The wire `emoji_index`: the tile index to add, or its bitwise NOT to remove.
pub fn encode_wire_index(index: i32, remove: bool) -> i32 {
    if remove {
        !index
    } else {
        index
    }
}

/// The tile index and whether it is a remove, from a wire `emoji_index`.
pub fn decode_wire_index(wire: i32) -> (i32, bool) {
    if wire < 0 {
        (!wire, true)
    } else {
        (wire, false)
    }
}

/// A received reaction's emoji and whether it is a remove. The emoji is the bevy-private `emoji`
/// field when that holds one emoji, else the atlas tile's; a remove is flagged by the bevy-private
/// `remove` field or a negative index.
pub fn received_emoji(
    emoji: &str,
    wire_index: Option<i32>,
    remove: bool,
) -> Option<(String, bool)> {
    let tile = wire_index.map(decode_wire_index);
    let emoji = canonical_emoji(emoji).or_else(|| {
        tile.and_then(|(index, _)| atlas_index_to_emoji(index))
            .map(ToOwned::to_owned)
    })?;
    Some((emoji, remove || tile.is_some_and(|(_, negative)| negative)))
}

/// `double.ToString(CultureInfo.InvariantCulture)` as Unity's Mono runtime prints it: "G" with
/// 15 significant digits, trailing zeros dropped, scientific below 1e-4 and from 1e15.
fn dotnet_double_string(value: f64) -> String {
    if value == 0.0 {
        return "0".to_owned();
    }
    let sign = if value < 0.0 { "-" } else { "" };
    let scientific = format!("{:.14e}", value.abs());
    let (mantissa, exponent) = scientific.split_once('e').unwrap();
    let exponent = exponent.parse::<i32>().unwrap();
    let digits = mantissa.replace('.', "");
    let digits = digits.trim_end_matches('0');

    if (-4..15).contains(&exponent) {
        if exponent < 0 {
            let zeros = "0".repeat((-exponent - 1) as usize);
            format!("{sign}0.{zeros}{digits}")
        } else {
            let integer_len = exponent as usize + 1;
            if digits.len() <= integer_len {
                let zeros = "0".repeat(integer_len - digits.len());
                format!("{sign}{digits}{zeros}")
            } else {
                let (integer, fraction) = digits.split_at(integer_len);
                format!("{sign}{integer}.{fraction}")
            }
        }
    } else {
        let (first, rest) = digits.split_at(1);
        let fraction = if rest.is_empty() {
            String::new()
        } else {
            format!(".{rest}")
        };
        let exponent_sign = if exponent < 0 { '-' } else { '+' };
        format!(
            "{sign}{first}{fraction}E{exponent_sign}{:02}",
            exponent.abs()
        )
    }
}

/// Unity's id for a chat message: the sender and the rfc4 `Chat` timestamp.
pub fn chat_message_id(sender: Address, timestamp: f64) -> String {
    format!("{sender:#x}:{}", dotnet_double_string(timestamp))
}

/// A received message id in our form, or `None` if it isn't `<address>:<timestamp>`. Parsing and
/// re-printing the timestamp matches however the sending client printed it.
pub fn canonical_message_id(id: &str) -> Option<String> {
    let (sender, timestamp) = id.split_once(':')?;
    let timestamp = timestamp.parse().ok().filter(|t: &f64| t.is_finite())?;
    Some(chat_message_id(sender.as_h160()?, timestamp))
}

/// A reaction added or removed, by the local user or a peer. `channel` is "Nearby" or the DM
/// partner's wallet.
#[derive(Event, Debug, Clone)]
pub struct ChatReactionEvent {
    pub channel: String,
    pub message_id: String,
    pub emoji: String,
    pub from: Address,
    pub remove: bool,
}

pub struct ChatReactionPlugin;

impl Plugin for ChatReactionPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<ChatReactionEvent>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dotnet_formatting() {
        assert_eq!(dotnet_double_string(46304.512345678901), "46304.5123456789");
        assert_eq!(dotnet_double_string(25569.0), "25569");
        assert_eq!(dotnet_double_string(0.5), "0.5");
        assert_eq!(dotnet_double_string(0.0001), "0.0001");
        assert_eq!(dotnet_double_string(0.00001), "1E-05");
        assert_eq!(dotnet_double_string(1e20), "1E+20");
        assert_eq!(dotnet_double_string(123456789012345.0), "123456789012345");
        assert_eq!(dotnet_double_string(-1.5), "-1.5");
    }

    #[test]
    fn message_ids_round_trip() {
        let sender = "0xAbCdEf0123456789abcdef0123456789ABCDEF01"
            .as_h160()
            .unwrap();
        let id = chat_message_id(sender, 46304.51234567129);
        assert_eq!(
            id,
            "0xabcdef0123456789abcdef0123456789abcdef01:46304.5123456713"
        );
        assert_eq!(canonical_message_id(&id).as_deref(), Some(id.as_str()));
        // a shortest-round-trip printing of the same timestamp maps to the same id
        assert_eq!(
            canonical_message_id("0xABCDEF0123456789abcdef0123456789abcdef01:46304.51234567129")
                .as_deref(),
            Some(id.as_str())
        );
        assert_eq!(canonical_message_id("not-an-id"), None);
        for timestamp in ["inf", "-infinity", "nan", "1e999"] {
            let id = format!("0xABCDEF0123456789abcdef0123456789abcdef01:{timestamp}");
            assert_eq!(canonical_message_id(&id), None, "{timestamp}");
        }
    }

    #[test]
    fn atlas_mapping() {
        assert_eq!(UNITY_EMOJI_ATLAS.lines().count(), 3576);
        // single codepoints, fe0f, skin tones and zwj sequences have their own tiles
        for emoji in ["👍", "❤️", "👍🏽", "🏃‍♂️", "#️⃣"] {
            let index = emoji_to_atlas_index(emoji);
            assert_ne!(index, ATLAS.unmapped, "{emoji}");
            assert_eq!(atlas_index_to_emoji(index), Some(emoji));
        }
        // unity has no flags
        assert_eq!(emoji_to_atlas_index("🇬🇧"), ATLAS.unmapped);
        assert_eq!(atlas_index_to_emoji(ATLAS.unmapped), Some(UNMAPPED));
        // unity's picker sends a keycap as its first codepoint
        let hash = ATLAS.tiles["0023"];
        assert_eq!(atlas_index_to_emoji(hash), Some("#️⃣"));
        // unity's own tile has no emoji
        assert_eq!(atlas_index_to_emoji(3575), Some(UNMAPPED));
        assert_eq!(atlas_index_to_emoji(3576), None);
        assert_eq!(atlas_index_to_emoji(-1), None);
    }

    #[test]
    fn received_emojis() {
        let thumbs = emoji_to_atlas_index("👍");
        assert_eq!(
            received_emoji("", Some(encode_wire_index(thumbs, true)), false),
            Some(("👍".to_owned(), true))
        );
        // the emoji field wins over the fallback tile
        assert_eq!(
            received_emoji("🇬🇧", Some(ATLAS.unmapped), false),
            Some(("🇬🇧".to_owned(), false))
        );
        // the emoji and remove flag alone, without a tile
        assert_eq!(
            received_emoji("🇬🇧", None, true),
            Some(("🇬🇧".to_owned(), true))
        );
        // a field that isn't one emoji falls back to the tile
        assert_eq!(
            received_emoji("not an emoji", Some(thumbs), false),
            Some(("👍".to_owned(), false))
        );
        assert_eq!(received_emoji("", Some(99999), false), None);
        assert_eq!(received_emoji("", None, false), None);
    }
}
