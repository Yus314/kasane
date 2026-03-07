#[cfg(feature = "perf-tracing")]
macro_rules! perf_span {
    ($name:expr) => {
        let _span = tracing::info_span!($name).entered();
    };
}

#[cfg(not(feature = "perf-tracing"))]
macro_rules! perf_span {
    ($name:expr) => {};
}

pub(crate) use perf_span;
