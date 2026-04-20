use easy_sat_rs::DimacsError;
use easy_sat_rs::{SolverBuilder, SolverStatus, search::Searcher};
use rstest::rstest;

#[rstest]
#[case("uf20-01.cnf", SolverStatus::UNSAT)]
#[case("uf20-02.cnf", SolverStatus::UNSAT)]
#[case("uf20-03.cnf", SolverStatus::UNSAT)]
#[case("uf20-04.cnf", SolverStatus::UNSAT)]
#[case("uf20-05.cnf", SolverStatus::UNSAT)]
#[case("uf20-06.cnf", SolverStatus::UNSAT)]
fn test_unsat_cnf(
    #[case] cnf_file: &str,
    #[case] expected_result: SolverStatus,
) -> Result<(), DimacsError> {
    use easy_sat_rs::SolveResult;

    let cnf_path = format!("./benchmarks/{}", cnf_file);
    let searcher = Searcher;
    let solver = SolverBuilder::from_dimacs_file(searcher, &cnf_path)?.build();

    let result = solver.solve();
    assert_eq!(result.status(), expected_result);
    match result {
        SolveResult::UNSAT(_) => {}
        _ => {
            panic!("expected UNSAT");
        }
    }
    Ok(())
}

#[rstest]
#[case("prime4.cnf", SolverStatus::SAT)]
fn test_sat_cnf(
    #[case] cnf_file: &str,
    #[case] expected_result: SolverStatus,
) -> Result<(), DimacsError> {
    use easy_sat_rs::SolveResult;

    let cnf_path = format!("./benchmarks/{}", cnf_file);
    let searcher = Searcher;
    let solver = SolverBuilder::from_dimacs_file(searcher, &cnf_path)?.build();

    let result = solver.solve();
    assert_eq!(result.status(), expected_result);
    match result {
        SolveResult::SAT(solver) => {
            assert!(solver.check_sat().is_ok());
            solver.print_model();
        }
        _ => {
            panic!("expected SAT");
        }
    }

    Ok(())
}
