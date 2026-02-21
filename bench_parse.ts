// Synthetic microbenchmark: JSON parse + mid price computation
// Replays a captured bookTicker message 1M times

const SAMPLE_MSG = JSON.stringify({
	stream: "btcusdt@bookTicker",
	data: {
		e: "bookTicker",
		E: 1700000000000,
		T: 1700000000000,
		s: "BTCUSDT",
		b: "43567.80",
		B: "1.234",
		a: "43567.90",
		A: "2.345",
	},
});

const ITERATIONS = 1_000_000;

// Warm up JIT
for (let i = 0; i < 10000; i++) {
	const msg = JSON.parse(SAMPLE_MSG) as { data: { b: string; a: string } };
	const bid = parseFloat(msg.data.b);
	const ask = parseFloat(msg.data.a);
	const _mid = (bid + ask) / 2;
}

const t0 = process.hrtime.bigint();

let sum = 0;
for (let i = 0; i < ITERATIONS; i++) {
	const msg = JSON.parse(SAMPLE_MSG) as { data: { b: string; a: string } };
	const bid = parseFloat(msg.data.b);
	const ask = parseFloat(msg.data.a);
	const mid = (bid + ask) / 2;
	sum += mid; // prevent dead code elimination
}

const elapsed = Number(process.hrtime.bigint() - t0);
const elapsedMs = elapsed / 1e6;
const perIterNs = elapsed / ITERATIONS;

console.log(`TypeScript JSON parse benchmark`);
console.log(`  Iterations: ${ITERATIONS.toLocaleString()}`);
console.log(`  Total time: ${elapsedMs.toFixed(1)} ms`);
console.log(`  Per iteration: ${perIterNs.toFixed(0)} ns`);
console.log(`  Throughput: ${((ITERATIONS / elapsedMs) * 1000).toFixed(0).replace(/\B(?=(\d{3})+(?!\d))/g, ",")} ops/sec`);
console.log(`  (sum=${sum} to prevent DCE)`);
