//! Build-time information display helpers for the CLI.

use shiotsuchi_core::build_info::{FEATURE_ASYNC_INDEX, FEATURE_WATCHER, HAS_MODEL_EMBEDDED};

fn watcher_status() -> &'static str {
    if FEATURE_WATCHER {
        "enabled"
    } else {
        "disabled"
    }
}

fn async_index_status() -> &'static str {
    if FEATURE_ASYNC_INDEX {
        "enabled"
    } else {
        "disabled"
    }
}

fn model_embedded_status() -> &'static str {
    if HAS_MODEL_EMBEDDED {
        "yes"
    } else {
        "no"
    }
}

pub fn help_footer() -> String {
    format!(
        "Build features: watcher={}, async-index={}, model-embedded={}",
        watcher_status(),
        async_index_status(),
        model_embedded_status()
    )
}

pub fn long_version() -> String {
    format!(
        "{}\nGuiding your path through the data tide.\nBuild features: watcher={}, async-index={}, model-embedded={}",
        env!("CARGO_PKG_VERSION"),
        watcher_status(),
        async_index_status(),
        model_embedded_status()
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn help_footer_contains_watcher_status() {
        let s = crate::build_info::help_footer();
        assert!(s.contains("watcher="));
    }

    #[test]
    fn long_version_contains_pkg_version_and_watcher() {
        let s = crate::build_info::long_version();
        assert!(s.contains(env!("CARGO_PKG_VERSION")));
        assert!(s.contains("watcher="));
    }
}
