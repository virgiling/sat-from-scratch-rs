all: fuzzer dd

fuzzer: cnfuzz.c
	cc -o fuzzer -O3 cnfuzz.c

dd: cnfdd.c
	cc -o delta -O3 cnfdd.c

clean:
	rm -f fuzzer delta