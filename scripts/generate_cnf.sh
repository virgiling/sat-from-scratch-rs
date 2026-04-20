#!/bin/bash

fuzzer=./third-party/cnfuzzdd/fuzzer

benchmark_dir=./benchmarks

run_fuzzer() {
    local benchmark_name=$1
    local seed=$2
    local benchmark_file=$benchmark_dir/$benchmark_name
    "$fuzzer" "$seed" > "${benchmark_file}.cnf"
}

run_fuzzer "uf20-01" 1
run_fuzzer "uf20-02" 2
run_fuzzer "uf20-03" 3
run_fuzzer "uf20-04" 4
run_fuzzer "uf20-05" 5
run_fuzzer "uf20-06" 6