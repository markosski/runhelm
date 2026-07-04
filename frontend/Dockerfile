FROM node:20-alpine AS builder

WORKDIR /app
COPY package*.json ./
COPY tsconfig*.json ./
COPY vite.config.ts ./
COPY index.html ./
COPY src ./src
RUN npm ci
RUN npm run build

FROM nginx:1.27-alpine

LABEL org.opencontainers.image.source="https://github.com/markosski/runhelm" \
      org.opencontainers.image.description="RunHelm frontend" \
      org.opencontainers.image.licenses="MIT"

COPY --from=builder /app/dist /usr/share/nginx/html

EXPOSE 80
