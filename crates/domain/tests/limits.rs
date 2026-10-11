//! Host-supplied resource policies preserve units and reject invalid bounds.
use gyral_renderer_domain::{LimitsError, RenderLimits};
use proptest::prelude::*;
use std::time::Duration;

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 128,
        rng_seed: proptest::test_runner::RngSeed::Fixed(0x006c_696d_6974),
        ..ProptestConfig::default()
    })]
    #[test]
    fn accepted_policies_preserve_the_host_values(render in 1..=60_000_u64, shutdown in 1..=60_000_u64, heap in 16*1024*1024..=1024*1024*1024_usize) {
        let limits = RenderLimits::try_new(Duration::from_millis(render), heap, Duration::from_millis(shutdown))?;
        prop_assert_eq!(limits.render_deadline(), Duration::from_millis(render));
        prop_assert_eq!(limits.shutdown_deadline(), Duration::from_millis(shutdown));
        prop_assert_eq!(limits.heap_bytes(), heap);
    }
}

#[test]
fn limits_accept_inclusive_boundaries_and_the_default() -> Result<(), LimitsError> {
    let minimum = RenderLimits::try_new(
        Duration::from_millis(1),
        16 * 1024 * 1024,
        Duration::from_millis(1),
    )?;
    let maximum = RenderLimits::try_new(
        Duration::from_secs(60),
        1024 * 1024 * 1024,
        Duration::from_secs(60),
    )?;
    assert_eq!(minimum.render_deadline(), Duration::from_millis(1));
    assert_eq!(minimum.shutdown_deadline(), Duration::from_millis(1));
    assert_eq!(minimum.heap_bytes(), 16 * 1024 * 1024);
    assert_eq!(maximum.render_deadline(), Duration::from_secs(60));
    assert_eq!(maximum.shutdown_deadline(), Duration::from_secs(60));
    assert_eq!(maximum.heap_bytes(), 1024 * 1024 * 1024);
    assert_eq!(
        RenderLimits::default(),
        RenderLimits::try_new(
            Duration::from_secs(2),
            64 * 1024 * 1024,
            Duration::from_secs(2)
        )?
    );
    Ok(())
}

#[test]
fn invalid_fields_return_their_typed_error() {
    for duration in [
        Duration::ZERO,
        Duration::from_nanos(999_999),
        Duration::from_secs(60) + Duration::from_nanos(1),
    ] {
        assert_eq!(
            RenderLimits::try_new(duration, 64 * 1024 * 1024, Duration::from_secs(2)),
            Err(LimitsError::RenderDeadline { duration })
        );
        assert_eq!(
            RenderLimits::try_new(Duration::from_secs(2), 64 * 1024 * 1024, duration),
            Err(LimitsError::ShutdownDeadline { duration })
        );
    }
    for bytes in [0, 16 * 1024 * 1024 - 1, 1024 * 1024 * 1024 + 1, usize::MAX] {
        assert_eq!(
            RenderLimits::try_new(Duration::from_secs(2), bytes, Duration::from_secs(2)),
            Err(LimitsError::HeapBudget { bytes })
        );
    }
}
