//! Build-time information display helpers for the CLI.

use shiotsuchi_core::build_info::{
    FEATURE_ASYNC_INDEX, FEATURE_PDF, FEATURE_VLM, FEATURE_WATCHER, HAS_MODEL_EMBEDDED,
};
use std::sync::LazyLock;

static HELP_FOOTER: LazyLock<String> = LazyLock::new(|| {
    format!(
        "Build features: watcher={}, async-index={}, model-embedded={}, pdf={}, vlm={}",
        watcher_status(),
        async_index_status(),
        model_embedded_status(),
        pdf_status(),
        vlm_status()
    )
});

static LONG_VERSION: LazyLock<String> = LazyLock::new(|| {
    format!(
        "{}\nGuiding your path through the data tide.\nBuild features: watcher={}, async-index={}, model-embedded={}, pdf={}, vlm={}",
        env!("CARGO_PKG_VERSION"),
        watcher_status(),
        async_index_status(),
        model_embedded_status(),
        pdf_status(),
        vlm_status()
    )
});

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

fn pdf_status() -> &'static str {
    if FEATURE_PDF {
        "enabled"
    } else {
        "disabled"
    }
}

fn vlm_status() -> &'static str {
    if FEATURE_VLM {
        "enabled"
    } else {
        "disabled"
    }
}

pub fn help_footer() -> &'static str {
    &HELP_FOOTER
}

pub fn long_version() -> &'static str {
    &LONG_VERSION
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
