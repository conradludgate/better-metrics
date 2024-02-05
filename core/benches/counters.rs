use divan::{AllocProfiler, Divan};

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

fn main() {
    Divan::from_args().threads([0]).run_benches();
}

mod fixed_cardinality {
    use divan::{black_box, Bencher};
    use lasso::{Rodeo, RodeoReader};
    use measured_derive::{FixedCardinalityLabel, LabelGroup};

    const LOOPS: usize = 2000;

    #[divan::bench(sample_size = 5, sample_count = 500)]
    fn measured(bencher: Bencher) {
        use measured::metric::name::{MetricName, Total};

        let error_set = ErrorsSet {
            route: Rodeo::from_iter(routes()).into_reader(),
        };
        let counter_vec = measured::CounterVec::new(error_set);

        bencher
            .with_inputs(measured::text::TextEncoder::new)
            .bench_refs(|encoder| {
                for _ in 0..black_box(LOOPS) {
                    for &kind in errors() {
                        for route in routes() {
                            counter_vec.inc(Error { kind, route })
                        }
                    }
                }

                let metric = "http_request_errors".with_suffix(Total);
                encoder.write_help(&metric, "help text");
                counter_vec.collect_into(&metric, encoder);
                encoder.finish();
            });
    }

    #[divan::bench(sample_size = 5, sample_count = 500)]
    fn measured_sparse(bencher: Bencher) {
        use measured::metric::name::{MetricName, Total};

        let error_set = ErrorsSet {
            route: Rodeo::from_iter(routes()).into_reader(),
        };
        let counter_vec = measured::CounterVec::new_sparse(error_set);

        bencher
            .with_inputs(measured::text::TextEncoder::new)
            .bench_refs(|encoder| {
                for _ in 0..black_box(LOOPS) {
                    for &kind in errors() {
                        for route in routes() {
                            counter_vec.inc(Error { kind, route })
                        }
                    }
                }

                let metric = "http_request_errors".with_suffix(Total);
                encoder.write_help(&metric, "help text");
                counter_vec.collect_into(&metric, encoder);
                encoder.finish();
            });
    }

    #[divan::bench(sample_size = 5, sample_count = 500)]
    fn prometheus(bencher: Bencher) {
        let registry = prometheus::Registry::new();
        let counter_vec = prometheus::register_int_counter_vec_with_registry!(
            "http_request_errors",
            "help text",
            &["kind", "route"],
            registry
        )
        .unwrap();

        bencher.with_inputs(String::new).bench_refs(|string| {
            for _ in 0..black_box(LOOPS) {
                for &kind in errors() {
                    for route in routes() {
                        counter_vec.with_label_values(&[kind.to_str(), route]).inc()
                    }
                }
            }

            string.clear();
            prometheus::TextEncoder
                .encode_utf8(&registry.gather(), string)
                .unwrap();
        });
    }

    #[divan::bench(sample_size = 5, sample_count = 500)]
    fn metrics(bencher: Bencher) {
        let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();

        metrics::with_local_recorder(&recorder, || {
            metrics::describe_counter!("http_request_errors", "help text")
        });

        bencher.bench(|| {
            metrics::with_local_recorder(&recorder, || {
                for _ in 0..black_box(LOOPS) {
                    for &kind in errors() {
                        for route in routes() {
                            let labels = [("kind", kind.to_str()), ("route", route)];
                            metrics::counter!("http_request_errors", &labels).increment(1);
                        }
                    }
                }
            });

            recorder.handle().render()
        });
    }

    fn routes() -> &'static [&'static str] {
        black_box(&[
            "/api/v1/users",
            "/api/v1/users/:id",
            "/api/v1/products",
            "/api/v1/products/:id",
            "/api/v1/products/:id/owner",
            "/api/v1/products/:id/purchase",
        ])
    }

    fn errors() -> &'static [ErrorKind] {
        black_box(&[ErrorKind::User, ErrorKind::Internal, ErrorKind::Network])
    }

    #[derive(Clone, Copy, PartialEq, Debug, LabelGroup)]
    #[label(set = ErrorsSet)]
    struct Error<'a> {
        #[label(fixed)]
        kind: ErrorKind,
        #[label(fixed_with = RodeoReader)]
        route: &'a str,
    }

    #[derive(Clone, Copy, PartialEq, Debug, FixedCardinalityLabel)]
    #[label(rename_all = "kebab-case")]
    enum ErrorKind {
        User,
        Internal,
        Network,
    }

    impl ErrorKind {
        fn to_str(self) -> &'static str {
            match self {
                ErrorKind::User => "user",
                ErrorKind::Internal => "internal",
                ErrorKind::Network => "network",
            }
        }
    }
}

mod high_cardinality {
    use std::sync::atomic::{AtomicU64, Ordering};

    use divan::{black_box, Bencher};
    use fake::{faker::name::raw::Name, locales::EN, Fake};
    use lasso::{Rodeo, RodeoReader, ThreadedRodeo};
    use measured_derive::{FixedCardinalityLabel, LabelGroup};
    use metrics::SharedString;
    use rand::{rngs::StdRng, SeedableRng};

    const LOOPS: usize = 1000;

    fn get_names(thread: &AtomicU64) -> Vec<String> {
        let extra = errors().len() * routes().len();
        let mut rng = StdRng::seed_from_u64(thread.fetch_add(1, Ordering::AcqRel));
        std::iter::repeat_with(|| Name(EN).fake_with_rng::<String, StdRng>(&mut rng))
            .take(LOOPS * extra)
            .collect()
    }

    #[divan::bench(sample_size = 2, sample_count = 20)]
    fn measured(bencher: Bencher) {
        use measured::metric::name::{MetricName, Total};

        let error_set = ErrorsSet {
            route: Rodeo::from_iter(routes()).into_reader(),
            user_name: ThreadedRodeo::new(),
        };
        let counter_vec = measured::CounterVec::new(error_set);

        let thread = AtomicU64::new(0);

        bencher
            .with_inputs(|| (measured::text::TextEncoder::new(), get_names(&thread)))
            .bench_refs(|(encoder, names)| {
                let mut names = names.iter();
                for _ in 0..black_box(LOOPS) {
                    for &kind in errors() {
                        for route in routes() {
                            counter_vec.inc(Error {
                                kind,
                                route,
                                user_name: names.next().unwrap(),
                            })
                        }
                    }
                }

                let metric = "http_request_errors".with_suffix(Total);
                encoder.write_help(&metric, "help text");
                counter_vec.collect_into(&metric, encoder);
                encoder.finish();
            });
    }

    #[divan::bench(sample_size = 2, sample_count = 20)]
    fn prometheus(bencher: Bencher) {
        let registry = prometheus::Registry::new();
        let counter_vec = prometheus::register_int_counter_vec_with_registry!(
            "http_request_errors_total",
            "help text",
            &["kind", "route", "user_name"],
            registry
        )
        .unwrap();

        let thread = AtomicU64::new(0);

        bencher
            .with_inputs(|| (String::new(), get_names(&thread)))
            .bench_refs(|(string, names)| {
                let mut names = names.iter();
                for _ in 0..black_box(LOOPS) {
                    for &kind in errors() {
                        for route in routes() {
                            counter_vec
                                .with_label_values(&[kind.to_str(), route, &names.next().unwrap()])
                                .inc()
                        }
                    }
                }

                string.clear();
                prometheus::TextEncoder
                    .encode_utf8(&registry.gather(), string)
                    .unwrap();
            });
    }

    #[divan::bench(sample_size = 2, sample_count = 20)]
    fn metrics(bencher: Bencher) {
        let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();

        metrics::with_local_recorder(&recorder, || {
            metrics::describe_counter!("http_request_errors", "help text")
        });

        let thread = AtomicU64::new(0);

        bencher
            .with_inputs(|| get_names(&thread))
            .bench_refs(|names| {
                let mut names = names.iter();
                metrics::with_local_recorder(&recorder, || {
                    for _ in 0..black_box(LOOPS) {
                        for &kind in errors() {
                            for route in routes() {
                                let labels = [
                                    ("kind", SharedString::const_str(kind.to_str())),
                                    ("route", SharedString::const_str(route)),
                                    (
                                        "user_name",
                                        SharedString::from_owned(names.next().unwrap().to_owned()),
                                    ),
                                ];
                                metrics::counter!("http_request_errors", &labels).increment(1);
                            }
                        }
                    }
                });

                recorder.handle().render()
            });
    }

    fn routes() -> &'static [&'static str] {
        black_box(&[
            "/api/v1/users",
            "/api/v1/users/:id",
            "/api/v1/products",
            "/api/v1/products/:id",
            "/api/v1/products/:id/owner",
            "/api/v1/products/:id/purchase",
        ])
    }

    fn errors() -> &'static [ErrorKind] {
        black_box(&[ErrorKind::User, ErrorKind::Internal, ErrorKind::Network])
    }

    #[derive(Clone, Copy, PartialEq, Debug, LabelGroup)]
    #[label(set = ErrorsSet)]
    struct Error<'a> {
        #[label(fixed)]
        kind: ErrorKind,
        #[label(fixed_with = RodeoReader)]
        route: &'a str,
        #[label(dynamic_with = ThreadedRodeo)]
        user_name: &'a str,
    }

    #[derive(Clone, Copy, PartialEq, Debug, FixedCardinalityLabel)]
    #[label(rename_all = "kebab-case")]
    enum ErrorKind {
        User,
        Internal,
        Network,
    }

    impl ErrorKind {
        fn to_str(self) -> &'static str {
            match self {
                ErrorKind::User => "user",
                ErrorKind::Internal => "internal",
                ErrorKind::Network => "network",
            }
        }
    }
}