import assert from 'node:assert/strict';
import test from 'node:test';
import { parseYahooChartQuote } from '../dist/adapters/executors/agent_tools/stockQuoteTool.js';

test('parses latest close and trend from Yahoo chart data', () => {
    const quote = parseYahooChartQuote({
        chart: {
            result: [{
                meta: {
                    currency: 'USD',
                    exchangeName: 'NMS',
                    exchangeTimezoneName: 'America/New_York',
                    regularMarketPrice: 211.26,
                    regularMarketTime: 1717780800,
                    symbol: 'AAPL',
                },
                timestamp: [1717608000, 1717694400, 1717780800],
                indicators: {
                    quote: [{
                        close: [195.87, null, 196.89],
                    }],
                },
            }],
            error: null,
        },
    }, 'AAPL', 'https://example.test/aapl', new Date('2026-05-10T12:00:00.000Z'));

    assert.equal(quote.symbol, 'AAPL');
    assert.equal(quote.latestClose, 196.89);
    assert.equal(quote.latestCloseTime, '2024-06-07T17:20:00.000Z');
    assert.equal(quote.previousClose, 195.87);
    assert.equal(quote.trendFromPreviousClose, 'up');
    assert.equal(quote.sourceUrl, 'https://example.test/aapl');
    assert.equal(quote.requestedAt, '2026-05-10T12:00:00.000Z');
});

test('rejects chart responses without closing prices', () => {
    assert.throws(
        () => parseYahooChartQuote({
            chart: {
                result: [{
                    timestamp: [1717608000],
                    indicators: {
                        quote: [{
                            close: [null],
                        }],
                    },
                }],
                error: null,
            },
        }, 'MSFT', 'https://example.test/msft'),
        /did not include closing prices/
    );
});
