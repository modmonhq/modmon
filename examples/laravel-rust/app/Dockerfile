FROM composer:2@sha256:4d71c3c2109c61d5415544264b59ad4087e4c5b7244481723664138fd36d5040 AS dependencies
WORKDIR /app
COPY composer.json composer.lock ./
RUN composer install \
    --no-dev \
    --no-interaction \
    --no-progress \
    --no-scripts \
    --prefer-dist
COPY . .
RUN composer dump-autoload --no-dev --classmap-authoritative

FROM dunglas/frankenphp:1-php8.4-bookworm@sha256:a6e78fd9da1640784cb07d606e313ae9f6013b1de39741643f0c256ecd87c8ae
RUN install-php-extensions pdo_pgsql redis pcntl
WORKDIR /app
COPY --from=dependencies --chown=www-data:www-data /app /app
COPY Caddyfile /etc/caddy/Caddyfile
RUN mkdir -p \
      storage/framework/cache/data \
      storage/framework/sessions \
      storage/framework/views \
      storage/logs \
      bootstrap/cache \
    && chown -R www-data:www-data storage bootstrap/cache
USER www-data
EXPOSE 8080
CMD ["frankenphp", "run", "--config", "/etc/caddy/Caddyfile"]
