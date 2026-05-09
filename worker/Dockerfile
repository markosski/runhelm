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
ENV NODE_ENV=production
ENV HOME=/home/runhelm
RUN addgroup -g 10001 runhelm \
    && adduser -D -h /home/runhelm -u 10001 -G runhelm runhelm \
    && mkdir -p /home/runhelm/.runhelm

# Copy package.json and install production dependencies only
COPY package*.json ./
RUN npm install --omit=dev

# Copy the built output from the builder stage
COPY --from=builder /app/dist ./dist
RUN chown -R runhelm:runhelm /app /home/runhelm
USER runhelm

# Set the entrypoint to run the worker directly with node
ENTRYPOINT ["node", "dist/index.js"]
