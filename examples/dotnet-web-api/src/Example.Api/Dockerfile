FROM mcr.microsoft.com/dotnet/sdk:11.0-alpine@sha256:52556c0fb9d0f609ae9aeab9efeeba96f4acc2f7038f7696f2235ccbdbb8e290 AS restore
WORKDIR /source
COPY Example.Api.csproj packages.lock.json ./
RUN dotnet restore --locked-mode

FROM restore AS development
COPY . .
CMD ["dotnet", "watch", "run", "--no-launch-profile", "--urls", "http://+:8080"]

FROM development AS build
RUN dotnet publish \
    --configuration Release \
    --no-restore \
    --output /app \
    /p:UseAppHost=false

FROM mcr.microsoft.com/dotnet/aspnet:11.0-alpine@sha256:d39b72ad1534fa0faf52b488bffb0762df7ba269d120f0338713d4dda2a41bf5
WORKDIR /app
COPY --from=build /app .
USER $APP_UID
EXPOSE 8080
ENTRYPOINT ["dotnet", "Example.Api.dll"]
