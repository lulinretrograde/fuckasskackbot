pub const XP_COOLDOWN_SECS: u64 = 60;

// ── level math ────────────────────────────────────────────────────────────────

/// XP required to go from level `n` to `n+1`
pub fn xp_for_level(n: u64) -> u64 {
    5 * n * n + 50 * n + 100
}

/// Total XP required to reach level `n` from level 0
pub fn total_xp_for_level(n: u64) -> u64 {
    (0..n).map(xp_for_level).sum()
}

pub fn level_from_xp(total_xp: u64) -> u64 {
    let mut level = 0u64;
    loop {
        if total_xp < total_xp_for_level(level + 1) {
            return level;
        }
        level += 1;
    }
}

/// Returns `(xp_into_current_level, xp_needed_for_next_level)`
pub fn xp_progress(total_xp: u64) -> (u64, u64) {
    let level = level_from_xp(total_xp);
    let base = total_xp_for_level(level);
    (total_xp - base, xp_for_level(level))
}

pub fn progress_bar(current: u64, total: u64) -> String {
    const WIDTH: usize = 12;
    let filled = if total == 0 {
        0
    } else {
        ((current as f64 / total as f64) * WIDTH as f64).round() as usize
    }
    .min(WIDTH);
    format!("{}{}", "█".repeat(filled), "░".repeat(WIDTH - filled))
}
