FROM node:20-alpine AS builder

WORKDIR /app

# Copy package.json and tsconfig.json
COPY package*.json ./
COPY tsconfig.json ./

# Install dependencies (ignoring scripts for security if needed, but simple install is fine for skeleton)
RUN npm install

# Copy source code
COPY src ./src

# Build the TypeScript code
RUN npm run build

# Stage 2: Create the production image
FROM node:20-alpine

WORKDIR /app
ARG RUNHELM_PI_PACKAGES=""
ENV NODE_ENV=production
ENV HOME=/home/runhelm
ENV XDG_CACHE_HOME=/home/runhelm/.cache
ENV npm_config_cache=/home/runhelm/.cache/npm
ENV COREPACK_HOME=/home/runhelm/.cache/corepack
RUN apk add --no-cache bash git make g++ python3
RUN addgroup -g 10001 runhelm \
    && adduser -D -h /home/runhelm -u 10001 -G runhelm runhelm \
    && mkdir -p /app /home/runhelm/.runhelm /home/runhelm/.cache /workspaces \
    && chown runhelm:runhelm /app /home/runhelm /home/runhelm/.runhelm /home/runhelm/.cache /workspaces

# Copy package.json and install production dependencies only
COPY --chown=runhelm:runhelm package*.json ./

RUN npm install --omit=dev
RUN if [ -n "$RUNHELM_PI_PACKAGES" ]; then npm install --omit=dev --no-save $RUNHELM_PI_PACKAGES; fi
RUN corepack enable

# Copy the built output from the builder stage
COPY --from=builder --chown=runhelm:runhelm /app/dist ./dist

# Set the entrypoint to run the worker directly with node
USER runhelm
ENTRYPOINT ["node", "dist/index.js"]
