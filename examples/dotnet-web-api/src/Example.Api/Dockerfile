FROM mcr.microsoft.com/dotnet/sdk:10.0-alpine@sha256:620e765fe18186c08399f7aa978f79f04b6bbf0ee1b3b8a91e2d5c9619e59da1 AS restore
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

FROM mcr.microsoft.com/dotnet/aspnet:10.0-alpine@sha256:c4b29bf368004ad9076c1ab9bc91fb373561e3905b4345637e14e8b8c57e3be8
WORKDIR /app
COPY --from=build /app .
USER $APP_UID
EXPOSE 8080
ENTRYPOINT ["dotnet", "Example.Api.dll"]
