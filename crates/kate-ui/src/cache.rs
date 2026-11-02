// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: GPL-3.0-or-later

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

//! Caches keyed by a line number and a theme color.

use std::collections::HashMap;

/// Shaped values kept by display number and theme color.
#[derive(Clone, Debug)]
pub(crate) struct KeyedCache<T> {
    slots: HashMap<(u32, u32), T>,
}

impl<T> Default for KeyedCache<T> {
    fn default() -> Self {
        Self {
            slots: HashMap::new(),
        }
    }
}

impl<T> KeyedCache<T> {
    pub(crate) fn get(&self, number: u32, theme: u32) -> Option<&T> {
        self.slots.get(&(number, theme))
    }

    pub(crate) fn insert(&mut self, number: u32, theme: u32, value: T) {
        self.slots.insert((number, theme), value);
    }
}

/// Pack a color into the theme half of a gutter key.
///
/// Each channel is a byte. `1.0` is 255.
pub(crate) fn theme_bits(red: f32, green: f32, blue: f32, alpha: f32) -> u32 {
    fn byte(channel: f32) -> u32 {
        (channel.clamp(0.0, 1.0) * 255.0).round() as u32
    }
    (byte(alpha) << 24) | (byte(red) << 16) | (byte(green) << 8) | byte(blue)
}

#[cfg(test)]
mod tests {
    use super::{KeyedCache, theme_bits};

    #[test]
    fn gutter_cache_keeps_one_entry_per_number_and_theme() {
        let mut cache = KeyedCache::default();
        cache.insert(12, 1, "twelve");
        cache.insert(12, 2, "other theme");
        cache.insert(13, 1, "thirteen");
        assert_eq!(cache.get(12, 1).copied(), Some("twelve"));
        assert_eq!(cache.get(12, 2).copied(), Some("other theme"));
        assert_eq!(cache.get(13, 1).copied(), Some("thirteen"));
        assert!(cache.get(12, 3).is_none());
        assert!(cache.get(14, 1).is_none());
    }

    #[test]
    fn theme_bits_change_when_one_channel_changes() {
        assert_eq!(theme_bits(1.0, 0.0, 0.0, 1.0), 0xFFFF_0000);
        assert_eq!(theme_bits(0.0, 1.0, 0.0, 1.0), 0xFF00_FF00);
        assert_ne!(
            theme_bits(1.0, 0.0, 0.0, 1.0),
            theme_bits(1.0, 0.0, 0.0, 0.0)
        );
    }
}
