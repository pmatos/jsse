#!/bin/bash
JSSE=/home/igalia/pmatos/dev/jsse/target/release/jsse
BOA=$HOME/.cargo/bin/boa
NODE=/usr/bin/node
TIMEOUT=120

BENCHMARKS="bench_loop bench_fib bench_string bench_array bench_object bench_regex bench_closures bench_json"

printf "%-15s | %-12s | %-12s | %-12s\n" "Benchmark" "Node.js" "Boa" "JSSE"
printf "%-15s-+-%-12s-+-%-12s-+-%-12s\n" "---------------" "------------" "------------" "------------"

for bench in $BENCHMARKS; do
    file="/tmp/${bench}.js"

    # Node
    node_time=$( { timeout $TIMEOUT /usr/bin/time -f "%e" $NODE "$file" > /dev/null; } 2>&1 )
    if [ $? -ne 0 ]; then node_time="TIMEOUT"; fi

    # Boa
    boa_time=$( { timeout $TIMEOUT /usr/bin/time -f "%e" $BOA "$file" > /dev/null; } 2>&1 )
    if [ $? -ne 0 ]; then boa_time="TIMEOUT"; fi

    # JSSE
    jsse_time=$( { timeout $TIMEOUT /usr/bin/time -f "%e" $JSSE "$file" > /dev/null; } 2>&1 )
    if [ $? -ne 0 ]; then jsse_time="TIMEOUT"; fi

    printf "%-15s | %10ss | %10ss | %10ss\n" "$bench" "$node_time" "$boa_time" "$jsse_time"
done
