import { Type } from '@mariozechner/pi-ai';
import { logger } from '../../../utils/logger.js';

type YahooChartResponse = {
    chart?: {
        result?: Array<{
            meta?: {
                currency?: string;
                exchangeName?: string;
                exchangeTimezoneName?: string;
                regularMarketPrice?: number;
                regularMarketTime?: number;
                symbol?: string;
            };
            timestamp?: number[];
            indicators?: {
                quote?: Array<{
                    close?: Array<number | null>;
                }>;
            };
        }>;
        error?: {
            code?: string;
            description?: string;
        } | null;
    };
};

export type StockQuote = {
    symbol: string;
    currency: string | null;
    exchangeName: string | null;
    exchangeTimezoneName: string | null;
    latestClose: number;
    latestCloseTime: string;
    previousClose: number | null;
    trendFromPreviousClose: 'up' | 'down' | 'flat' | 'unknown';
    regularMarketPrice: number | null;
    regularMarketTime: string | null;
    sourceUrl: string;
    requestedAt: string;
};

export function parseYahooChartQuote(data: YahooChartResponse, requestedSymbol: string, sourceUrl: string, requestedAt = new Date()): StockQuote {
    if (data.chart?.error) {
        const description = data.chart.error.description || data.chart.error.code || 'unknown Yahoo Finance error';
        throw new Error(`Yahoo Finance chart error for ${requestedSymbol}: ${description}`);
    }

    const result = data.chart?.result?.[0];
    if (!result) {
        throw new Error(`Yahoo Finance chart response did not include quote data for ${requestedSymbol}`);
    }

    const timestamps = result.timestamp || [];
    const closes = result.indicators?.quote?.[0]?.close || [];
    const observedCloses = closes
        .map((close, index) => ({ close, timestamp: timestamps[index] }))
        .filter((point): point is { close: number; timestamp: number } =>
            typeof point.close === 'number' && Number.isFinite(point.close) &&
            typeof point.timestamp === 'number' && Number.isFinite(point.timestamp)
        );

    if (observedCloses.length === 0) {
        throw new Error(`Yahoo Finance chart response did not include closing prices for ${requestedSymbol}`);
    }

    const latest = observedCloses[observedCloses.length - 1];
    if (!latest) {
        throw new Error(`Yahoo Finance chart response did not include closing prices for ${requestedSymbol}`);
    }
    const previous = observedCloses.length > 1 ? observedCloses[observedCloses.length - 2] : undefined;
    const previousClose = previous?.close ?? null;

    let trendFromPreviousClose: StockQuote['trendFromPreviousClose'] = 'unknown';
    if (previousClose !== null) {
        if (latest.close > previousClose) {
            trendFromPreviousClose = 'up';
        } else if (latest.close < previousClose) {
            trendFromPreviousClose = 'down';
        } else {
            trendFromPreviousClose = 'flat';
        }
    }

    const meta = result.meta || {};
    return {
        symbol: meta.symbol || requestedSymbol.toUpperCase(),
        currency: meta.currency || null,
        exchangeName: meta.exchangeName || null,
        exchangeTimezoneName: meta.exchangeTimezoneName || null,
        latestClose: latest.close,
        latestCloseTime: new Date(latest.timestamp * 1000).toISOString(),
        previousClose,
        trendFromPreviousClose,
        regularMarketPrice: typeof meta.regularMarketPrice === 'number' ? meta.regularMarketPrice : null,
        regularMarketTime: typeof meta.regularMarketTime === 'number' ? new Date(meta.regularMarketTime * 1000).toISOString() : null,
        sourceUrl,
        requestedAt: requestedAt.toISOString(),
    };
}

export function createStockQuoteTool() {
    return {
        name: "stock_quote",
        description: "Fetch normalized recent stock quote data from Yahoo Finance chart data. Prefer this over raw page fetches for stock prices.",
        label: "Stock Quote",
        parameters: Type.Object({
            symbol: Type.String({ description: "Ticker symbol, for example AAPL or MSFT" })
        }),
        execute: async (toolCallId: string, args: any, signal?: AbortSignal) => {
            const symbol = String(args.symbol || '').trim().toUpperCase();
            if (!/^[A-Z0-9.^-]{1,20}$/.test(symbol)) {
                throw new Error(`Invalid stock symbol: ${args.symbol}`);
            }

            const sourceUrl = `https://query1.finance.yahoo.com/v8/finance/chart/${encodeURIComponent(symbol)}?range=1mo&interval=1d`;
            logger.info(`[StockQuoteTool] Fetching quote for ${symbol}`);

            const response = await fetch(sourceUrl, {
                headers: {
                    "Accept": "application/json",
                    "User-Agent": "runhelm-worker/1.0"
                },
                signal: signal || null
            });

            if (!response.ok) {
                throw new Error(`Yahoo Finance quote request failed with status ${response.status}: ${response.statusText}`);
            }

            const data = await response.json() as YahooChartResponse;
            const quote = parseYahooChartQuote(data, symbol, sourceUrl);

            return {
                content: [{ type: "text", text: JSON.stringify(quote, null, 2) }],
                details: quote
            };
        }
    } as any;
}
