// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: GPL-3.0-or-later

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

//! Caches keyed by a line number and a theme color.

use std::collections::HashMap;

/// Line numbers kept for one theme. A theme change drops the previous theme.
const GUTTER_CACHE_CAP: usize = 4_096;

/// Shaped values kept by display number and theme color.
#[derive(Clone, Debug)]
pub(crate) struct KeyedCache<T> {
    slots: HashMap<(u32, u32), T>,
    theme: Option<u32>,
}

impl<T> Default for KeyedCache<T> {
    fn default() -> Self {
        Self {
            slots: HashMap::new(),
            theme: None,
        }
    }
}

impl<T> KeyedCache<T> {
    pub(crate) fn get(&self, number: u32, theme: u32) -> Option<&T> {
        self.slots.get(&(number, theme))
    }

    pub(crate) fn insert(&mut self, number: u32, theme: u32, value: T) {
        if self.theme.is_some_and(|current| current != theme) {
            self.slots.clear();
        }
        self.theme = Some(theme);
        let key = (number, theme);
        if !self.slots.contains_key(&key) && self.slots.len() >= GUTTER_CACHE_CAP {
            self.slots.clear();
        }
        self.slots.insert(key, value);
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
    use super::{GUTTER_CACHE_CAP, KeyedCache, theme_bits};

    #[test]
    fn gutter_cache_keeps_one_entry_per_number_and_theme() {
        let mut cache = KeyedCache::default();
        cache.insert(12, 1, "twelve");
        cache.insert(13, 1, "thirteen");
        assert_eq!(cache.get(12, 1).copied(), Some("twelve"));
        assert_eq!(cache.get(13, 1).copied(), Some("thirteen"));
        assert!(cache.get(14, 1).is_none());
    }

    #[test]
    fn gutter_cache_drops_the_previous_theme() {
        let mut cache = KeyedCache::default();
        cache.insert(12, 1, "old");
        cache.insert(12, 2, "new");
        assert!(cache.get(12, 1).is_none());
        assert_eq!(cache.get(12, 2).copied(), Some("new"));
    }

    #[test]
    fn gutter_cache_caps_the_number_of_line_numbers() {
        let mut cache = KeyedCache::default();
        for number in 0..GUTTER_CACHE_CAP as u32 {
            cache.insert(number, 1, number);
        }
        assert_eq!(cache.get(0, 1).copied(), Some(0));
        cache.insert(GUTTER_CACHE_CAP as u32, 1, 99);
        assert!(cache.get(0, 1).is_none());
        assert_eq!(cache.get(GUTTER_CACHE_CAP as u32, 1).copied(), Some(99));
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
