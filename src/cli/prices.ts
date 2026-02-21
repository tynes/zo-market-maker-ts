// CLI entry point for monitoring Binance price feeds

import "dotenv/config";
import { BinancePriceFeed } from "../pricing/binance.js";
import { log } from "../utils/logger.js";

function main(): void {
	const symbols = process.argv.slice(2).map((s) => s.toLowerCase());

	if (symbols.length === 0) {
		console.error("Usage: npm run prices -- <symbol> [symbol...]");
		console.error("Example: npm run prices -- btcusdt ethusdt solusdt");
		process.exit(1);
	}

	const feeds: BinancePriceFeed[] = [];

	for (const symbol of symbols) {
		const feed = new BinancePriceFeed(symbol);
		feed.onPrice = (price) => {
			log.info(
				`${symbol.toUpperCase()} bid=$${price.bid} ask=$${price.ask} mid=$${price.mid}`,
			);
		};
		feed.connect();
		feeds.push(feed);
	}

	const shutdown = () => {
		log.shutdown();
		for (const feed of feeds) {
			feed.close();
		}
		process.exit(0);
	};

	process.on("SIGINT", shutdown);
	process.on("SIGTERM", shutdown);
}

main();
