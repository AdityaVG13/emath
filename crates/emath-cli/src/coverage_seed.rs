//! Seed dataset for the coverage ledger: one row per MSC super-domain.
//!
//! Imported from the Phase 3a MSC matrix (02 B01-B46) through the rating
//! vocabulary: FULL -> reference-impl, SYNTAX-ONLY -> contract, MISSING ->
//! none, PARTIAL -> per-facet split (expressed here as the per-facet rating
//! words themselves; a wholesale PARTIAL is never stored).
//!
//! Ratings are honest to what computes today: a FULL facet cites an artifact
//! (a runnable example under `language/examples/` or a laws pack under
//! `language/stdlib/laws/`); SYNTAX-ONLY facets are contracts the reference
//! defines but that do not compute end-to-end yet; MISSING facets are the
//! gap backlog (B-items) with no contract. Super-domain granularity is the
//! seed snapshot; sub-area (57-row) granularity upgrades as the matrix is
//! imported per row.

/// One MSC super-domain row. `ratings` is in `FACETS` order
/// (types, operators, goals, notation, worlds, evidence).
pub struct DomainSeed {
    pub msc: &'static str,
    pub super_domain: &'static str,
    pub label: &'static str,
    pub ratings: [&'static str; 6],
    /// Linked artifact per facet; required (non-None) wherever the facet
    /// rating maps to `reference-impl` or above.
    pub artifacts: [Option<&'static str>; 6],
    /// `PACKAGE_CATALOG.md` packages whose responsibility lives in this
    /// domain. Every catalog row must be claimed by exactly one domain; the
    /// generator fails otherwise (E-COV-PACKAGE-UNCLAIMED).
    pub packages: &'static [&'static str],
}

const NONE: Option<&str> = None;

/// 12 super-domains spanning MSC 2020 top-level codes.
pub const SEED: [DomainSeed; 12] = [
    DomainSeed {
        msc: "00-05",
        super_domain: "Foundations, logic, and generalities",
        label: "general math, logic, set theory as claim substrate",
        ratings: ["SYNTAX-ONLY", "MISSING", "SYNTAX-ONLY", "SYNTAX-ONLY", "MISSING", "FULL"],
        artifacts: [
            Some("language/reference/types-units-shapes-and-domains.md"),
            NONE,
            Some("language/reference/goals-requests-strategies-and-resolution.md"),
            Some("language/reference/lexical-layout-and-source.md"),
            NONE,
            Some("language/examples/science/observations.emath"),
        ],
        packages: &["std.core", "core::prelude", "core::logic", "core::collections", "core::evidence", "core::units"],
    },
    DomainSeed {
        msc: "08-13",
        super_domain: "Number theory and arithmetic",
        label: "modular arithmetic, exact integers, conjecture no-claims",
        ratings: ["FULL", "FULL", "SYNTAX-ONLY", "SYNTAX-ONLY", "MISSING", "FULL"],
        artifacts: [
            Some("language/stdlib/laws/algebra-number-theory.emath"),
            Some("language/stdlib/laws/algebra-number-theory.emath"),
            Some("language/reference/goals-requests-strategies-and-resolution.md"),
            Some("language/reference/lexical-layout-and-source.md"),
            NONE,
            Some("language/stdlib/laws/algebra-number-theory.emath"),
        ],
        packages: &["core::math", "core::numbers", "core::number_theory", "number_theory::laws"],
    },
    DomainSeed {
        msc: "14-20",
        super_domain: "Algebra and algebraic structures",
        label: "symbolic simplification, algebraic slices",
        ratings: ["FULL", "FULL", "MISSING", "SYNTAX-ONLY", "MISSING", "FULL"],
        artifacts: [
            Some("language/examples/algebra/symbolic-cas.emath"),
            Some("language/examples/algebra/symbolic-cas.emath"),
            NONE,
            Some("language/reference/expressions-equations-state-and-events.md"),
            NONE,
            Some("language/stdlib/laws/algebra-number-theory.emath"),
        ],
        packages: &["core::algebra"],
    },
    DomainSeed {
        msc: "22-27",
        super_domain: "Group theory, topology, and geometry",
        label: "abstract structures, manifolds, fields/forms",
        ratings: ["MISSING", "MISSING", "MISSING", "MISSING", "MISSING", "MISSING"],
        artifacts: [NONE, NONE, NONE, NONE, NONE, NONE],
        packages: &[],
    },
    DomainSeed {
        msc: "28-31",
        super_domain: "Measures, integration, and probability foundations",
        label: "measures, general integrals, measure-theoretic probability",
        ratings: ["MISSING", "MISSING", "MISSING", "MISSING", "MISSING", "MISSING"],
        artifacts: [NONE, NONE, NONE, NONE, NONE, NONE],
        packages: &["core::domains"],
    },
    DomainSeed {
        msc: "33-35",
        super_domain: "Analysis, special functions, and ODEs",
        label: "endpoint/Taylor/contraction slices, forward AD, ODE solves",
        ratings: ["FULL", "FULL", "FULL", "SYNTAX-ONLY", "MISSING", "FULL"],
        artifacts: [
            Some("language/stdlib/laws/analysis.emath"),
            Some("language/examples/intro/autodiff.emath"),
            Some("language/examples/intro/autodiff.emath"),
            Some("language/reference/expressions-equations-state-and-events.md"),
            NONE,
            Some("language/stdlib/laws/analysis.emath"),
        ],
        packages: &["core::calculus", "core::state", "core::special_functions", "analysis::laws"],
    },
    DomainSeed {
        msc: "35, 76-80",
        super_domain: "Partial differential equations and continuum fields",
        label: "laplacian-based heat/gradient simulation, anisotropic tensors",
        ratings: ["FULL", "FULL", "FULL", "SYNTAX-ONLY", "MISSING", "FULL"],
        artifacts: [
            Some("language/examples/numerical/heat-rod-sim.emath"),
            Some("language/examples/numerical/heat-rod-sim.emath"),
            Some("language/examples/numerical/heat-rod-sim.emath"),
            Some("language/reference/expressions-equations-state-and-events.md"),
            NONE,
            Some("language/examples/numerical/heat-rod-sim.emath"),
        ],
        packages: &[],
    },
    DomainSeed {
        msc: "39-49",
        super_domain: "Finite mathematics, combinatorics, and optimization",
        label: "finite KKT, Bellman, Lyapunov slices; constraint goals",
        ratings: ["FULL", "FULL", "SYNTAX-ONLY", "MISSING", "MISSING", "FULL"],
        artifacts: [
            Some("language/stdlib/laws/optimization-control.emath"),
            Some("language/examples/intro/optimize.emath"),
            Some("language/examples/intro/optimize.emath"),
            NONE,
            NONE,
            Some("language/stdlib/laws/optimization-control.emath"),
        ],
        packages: &[
            "core::optimization",
            "core::combinatorics",
            "core::game_theory",
            "core::lp_milp",
            "optimization::methods",
            "optimization_control::laws",
        ],
    },
    DomainSeed {
        msc: "60-62",
        super_domain: "Probability and statistics",
        label: "finite Bayes, CLT scaling, information slices",
        ratings: ["FULL", "SYNTAX-ONLY", "SYNTAX-ONLY", "MISSING", "MISSING", "FULL"],
        artifacts: [
            Some("language/stdlib/laws/probability-statistics.emath"),
            Some("language/reference/expressions-equations-state-and-events.md"),
            Some("language/reference/goals-requests-strategies-and-resolution.md"),
            NONE,
            NONE,
            Some("language/stdlib/laws/probability-statistics.emath"),
        ],
        packages: &["core::probability", "probability::information", "probability::laws"],
    },
    DomainSeed {
        msc: "65",
        super_domain: "Numerical analysis and computation",
        label: "RK4/RK45 integrators, solvers, determinism class",
        ratings: ["FULL", "FULL", "FULL", "SYNTAX-ONLY", "SYNTAX-ONLY", "FULL"],
        artifacts: [
            Some("language/examples/numerical/explicit-mass-spring.emath"),
            Some("language/examples/numerical/explicit-mass-spring.emath"),
            Some("language/examples/numerical/explicit-mass-spring.emath"),
            Some("language/reference/diagnostics-and-tooling-contract.md"),
            Some("language/reference/total-compilation-protocol.md"),
            Some("language/stdlib/laws/computer-science.emath"),
        ],
        packages: &["core::shapes", "core::linear_algebra", "approximation::laws"],
    },
    DomainSeed {
        msc: "68, 97",
        super_domain: "Computer science and education-adjacent computation",
        label: "systems laws, open-problem deferrals, executable curricula",
        ratings: ["FULL", "FULL", "SYNTAX-ONLY", "SYNTAX-ONLY", "MISSING", "FULL"],
        artifacts: [
            Some("language/stdlib/laws/computer-science.emath"),
            Some("language/stdlib/laws/computer-science.emath"),
            Some("language/reference/goals-requests-strategies-and-resolution.md"),
            Some("language/reference/lexical-layout-and-source.md"),
            NONE,
            Some("language/stdlib/laws/computer-science.emath"),
        ],
        packages: &["core::graphs", "core::artifact", "core::host", "cs::laws"],
    },
    DomainSeed {
        msc: "70-86",
        super_domain: "Physics and mechanics",
        label: "classical mechanics laws, special relativity slice",
        ratings: ["FULL", "FULL", "FULL", "SYNTAX-ONLY", "MISSING", "FULL"],
        artifacts: [
            Some("language/stdlib/laws/physics-classical.emath"),
            Some("language/examples/physics/newton-second.emath"),
            Some("language/examples/physics/newton-second.emath"),
            Some("language/reference/expressions-equations-state-and-events.md"),
            NONE,
            Some("language/stdlib/laws/physics-relativity.emath"),
        ],
        packages: &["physics::classical", "physics::relativity"],
    },
];
