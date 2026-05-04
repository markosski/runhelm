import pino from "pino";

// Configure logger depending on environment.
// In development, we use pino-pretty for human-readable output.
// In production (Docker), we output raw JSON which is better for log aggregators.
const isDev = process.env.NODE_ENV !== "production";

export const logger = pino(
    {
        level: process.env.LOG_LEVEL || "info",
    },
    isDev
        ? pino.transport({
              target: "pino-pretty",
              options: {
                  colorize: true,
                  translateTime: "SYS:standard",
                  ignore: "pid,hostname",
                  destination: 2, // stderr
              },
          })
        : pino.destination(2) // stderr
);

