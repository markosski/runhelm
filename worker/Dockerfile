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

# Copy package.json and install production dependencies only
COPY package*.json ./
RUN npm install --omit=dev

# Copy the built output from the builder stage
COPY --from=builder /app/dist ./dist

# Set the entrypoint to run the worker directly with node
ENTRYPOINT ["node", "dist/index.js"]
