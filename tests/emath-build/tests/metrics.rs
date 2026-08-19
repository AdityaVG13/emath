mod metrics {
    use emath_build::MetricsCollector;

    #[test]
    fn receipt_format_is_byte_stable_for_the_same_recorded_values() {
        let mut collector = MetricsCollector::new();
        collector.record_duration_ns("check_plan", 1200);
        collector.record_duration_ns("artifact_pipeline", 3400);
        collector.record_count("plan_count", 2);
        collector.record_count("artifact_bytes", 999);
        let first = collector.benchmark_receipt("spec.emath", "fnv1a64:abc");
        let second = collector.benchmark_receipt("spec.emath", "fnv1a64:abc");
        assert_eq!(first, second);
        assert!(first.contains("\"schema\": \"emath.benchmark-receipt\""));
        assert!(first.contains("\"version\": 1"));
        assert!(first.contains("\"duration_ns.check_plan\": 1200"));
        assert!(first.contains("\"count.plan_count\": 2"));
        // Sorted key order: artifact_pipeline before check_plan,
        // artifact_bytes before plan_count.
        let pipeline = first.find("duration_ns.artifact_pipeline").unwrap();
        let check = first.find("duration_ns.check_plan").unwrap();
        assert!(pipeline < check, "duration keys must be sorted");
    }

    #[test]
    fn collectors_accumulate_re_entered_phases_and_counters() {
        let mut collector = MetricsCollector::new();
        collector.record_duration_ns("check_plan", 10);
        collector.record_duration_ns("check_plan", 5);
        collector.record_count("semantic_rejected", 1);
        collector.record_count("semantic_rejected", 2);
        let receipt = collector.benchmark_receipt("s", "a");
        assert!(receipt.contains("\"duration_ns.check_plan\": 15"));
        assert!(receipt.contains("\"count.semantic_rejected\": 3"));
    }
}
