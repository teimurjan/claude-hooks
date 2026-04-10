use std::collections::HashSet;
use std::env;

const VALID_PROFILES: [&str; 3] = ["minimal", "standard", "strict"];

fn hook_allowed_profiles(hook_id: &str) -> &'static [&'static str] {
    match hook_id {
        "session-start" | "cost-tracker" => &["minimal", "standard", "strict"],

        "pre-bash-tmux-reminder" | "pre-bash-git-push-reminder" => &["strict"],

        _ => &["standard", "strict"],
    }
}

fn active_profile() -> String {
    let raw = env::var("TEIMURJAN_HOOK_PROFILE").unwrap_or_default();
    let normalized = raw.trim().to_ascii_lowercase();
    if VALID_PROFILES.contains(&normalized.as_str()) {
        normalized
    } else {
        "standard".into()
    }
}

fn disabled_hook_ids() -> HashSet<String> {
    env::var("TEIMURJAN_DISABLED_HOOKS")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn is_hook_enabled(hook_id: &str) -> bool {
    let id = hook_id.trim().to_ascii_lowercase();
    if id.is_empty() {
        return true;
    }
    if disabled_hook_ids().contains(&id) {
        return false;
    }
    let profile = active_profile();
    hook_allowed_profiles(&id).contains(&profile.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_hooks_allowed_in_all_profiles() {
        for hook in ["session-start", "cost-tracker"] {
            let profiles = hook_allowed_profiles(hook);
            assert!(profiles.contains(&"minimal"), "{hook} should be in minimal");
            assert!(profiles.contains(&"standard"), "{hook} should be in standard");
            assert!(profiles.contains(&"strict"), "{hook} should be in strict");
        }
    }

    #[test]
    fn strict_only_hooks() {
        for hook in ["pre-bash-tmux-reminder", "pre-bash-git-push-reminder"] {
            let profiles = hook_allowed_profiles(hook);
            assert_eq!(profiles, &["strict"], "{hook} should be strict-only");
        }
    }

    #[test]
    fn default_hooks_in_standard_and_strict() {
        let profiles = hook_allowed_profiles("quality-gate");
        assert!(profiles.contains(&"standard"));
        assert!(profiles.contains(&"strict"));
        assert!(!profiles.contains(&"minimal"));
    }

    #[test]
    fn active_profile_defaults_to_standard() {
        unsafe { env::remove_var("TEIMURJAN_HOOK_PROFILE") };
        assert_eq!(active_profile(), "standard");
    }

    #[test]
    fn active_profile_invalid_falls_back() {
        unsafe { env::set_var("TEIMURJAN_HOOK_PROFILE", "ultra") };
        assert_eq!(active_profile(), "standard");
        unsafe { env::remove_var("TEIMURJAN_HOOK_PROFILE") };
    }

    #[test]
    fn empty_hook_id_always_enabled() {
        assert!(is_hook_enabled(""));
        assert!(is_hook_enabled("  "));
    }
}
