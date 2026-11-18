using Npgsql;
using System.Net.Mail;
using System.Net.Sockets;

var builder = WebApplication.CreateBuilder(args);
var database = builder.Configuration.GetRequiredSection("Database");
var connectionString = new NpgsqlConnectionStringBuilder
{
    Host = database["Host"] ?? throw new InvalidOperationException("Database:Host is required."),
    Port = database.GetValue("Port", 5432),
    Database = database["Name"] ?? throw new InvalidOperationException("Database:Name is required."),
    Username = database["Username"] ?? throw new InvalidOperationException("Database:Username is required."),
    Password = database["Password"] ?? throw new InvalidOperationException("Database:Password is required."),
    Pooling = true,
}.ConnectionString;

builder.Services.AddSingleton(_ => NpgsqlDataSource.Create(connectionString));

var app = builder.Build();
await InitializeDatabase(app.Services.GetRequiredService<NpgsqlDataSource>());

app.MapGet("/health", async (NpgsqlDataSource dataSource, CancellationToken cancellationToken) =>
{
    await using var command = dataSource.CreateCommand("SELECT 1");
    await command.ExecuteScalarAsync(cancellationToken);
    return Results.Ok(new { status = "ok" });
});

app.MapGet("/api/widgets", async (
    NpgsqlDataSource dataSource,
    CancellationToken cancellationToken) =>
{
    await using var command = dataSource.CreateCommand(
        "SELECT id, name, created_at FROM widgets ORDER BY id DESC LIMIT 100");
    await using var reader = await command.ExecuteReaderAsync(cancellationToken);
    var widgets = new List<Widget>();

    while (await reader.ReadAsync(cancellationToken))
    {
        widgets.Add(new Widget(
            reader.GetInt64(0),
            reader.GetString(1),
            reader.GetDateTime(2)));
    }

    return Results.Ok(widgets);
});

app.MapPost("/api/widgets", async (
    CreateWidget input,
    NpgsqlDataSource dataSource,
    CancellationToken cancellationToken) =>
{
    var name = input.Name?.Trim();
    if (string.IsNullOrEmpty(name) || name.Length > 100)
    {
        return Results.ValidationProblem(new Dictionary<string, string[]>
        {
            ["name"] = ["Name must contain between 1 and 100 characters."],
        });
    }

    await using var command = dataSource.CreateCommand(
        "INSERT INTO widgets (name) VALUES ($1) RETURNING id, created_at");
    command.Parameters.Add(new NpgsqlParameter { Value = name });
    await using var reader = await command.ExecuteReaderAsync(cancellationToken);
    await reader.ReadAsync(cancellationToken);

    var widget = new Widget(reader.GetInt64(0), name, reader.GetDateTime(1));
    return Results.Created($"/api/widgets/{widget.Id}", widget);
});

app.MapPost("/api/mail-preview", async (
    MailPreview input,
    IConfiguration configuration,
    CancellationToken cancellationToken) =>
{
    if (!MailAddress.TryCreate(input.To, out var recipient))
    {
        return Results.ValidationProblem(new Dictionary<string, string[]>
        {
            ["to"] = ["A valid recipient address is required."],
        });
    }

    var host = configuration["MM_MAIL_SMTP_HOST"];
    if (string.IsNullOrWhiteSpace(host))
    {
        return Results.Problem("Development mail is not configured.", statusCode: 503);
    }

    var port = configuration.GetValue("MM_MAIL_SMTP_PORT", 1025);
    await SendPreviewMail(host, port, recipient.Address, cancellationToken);
    return Results.Accepted(value: new
    {
        accepted = true,
        mailpit = configuration["MM_MAIL_UI_URL"],
    });
});

await app.RunAsync();

static async Task InitializeDatabase(NpgsqlDataSource dataSource)
{
    Exception? lastError = null;

    for (var attempt = 1; attempt <= 20; attempt++)
    {
        try
        {
            await using var command = dataSource.CreateCommand("""
                CREATE TABLE IF NOT EXISTS widgets (
                    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
                    name TEXT NOT NULL,
                    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
                )
                """);
            await command.ExecuteNonQueryAsync();
            return;
        }
        catch (NpgsqlException error)
        {
            lastError = error;
            if (attempt == 20)
            {
                break;
            }

            await Task.Delay(TimeSpan.FromMilliseconds(1500));
        }
    }

    throw new InvalidOperationException("PostgreSQL did not become ready.", lastError);
}

static async Task SendPreviewMail(
    string host,
    int port,
    string recipient,
    CancellationToken cancellationToken)
{
    using var client = new TcpClient();
    await client.ConnectAsync(host, port, cancellationToken);
    await using var stream = client.GetStream();
    using var reader = new StreamReader(stream, leaveOpen: true);
    await using var writer = new StreamWriter(stream, leaveOpen: true)
    {
        NewLine = "\r\n",
        AutoFlush = true,
    };

    await ExpectSmtpReply(reader, cancellationToken);
    foreach (var command in new[]
    {
        "EHLO api.dotnet-web-api.test",
        "MAIL FROM:<preview@dotnet-web-api.test>",
        $"RCPT TO:<{recipient}>",
        "DATA",
    })
    {
        await writer.WriteLineAsync(command.AsMemory(), cancellationToken);
        await ExpectSmtpReply(reader, cancellationToken);
    }

    await writer.WriteLineAsync("Subject: mm Mailpit preview".AsMemory(), cancellationToken);
    await writer.WriteLineAsync("From: preview@dotnet-web-api.test".AsMemory(), cancellationToken);
    await writer.WriteLineAsync($"To: {recipient}".AsMemory(), cancellationToken);
    await writer.WriteLineAsync(ReadOnlyMemory<char>.Empty, cancellationToken);
    await writer.WriteLineAsync("The .NET example reached shared Mailpit.".AsMemory(), cancellationToken);
    await writer.WriteLineAsync(".".AsMemory(), cancellationToken);
    await ExpectSmtpReply(reader, cancellationToken);
    await writer.WriteLineAsync("QUIT".AsMemory(), cancellationToken);
    await ExpectSmtpReply(reader, cancellationToken);
}

static async Task ExpectSmtpReply(StreamReader reader, CancellationToken cancellationToken)
{
    string? line;
    do
    {
        line = await reader.ReadLineAsync(cancellationToken);
        if (line is null || line.Length < 4)
        {
            throw new IOException("SMTP server closed without a complete response.");
        }
    }
    while (line[3] == '-');

    if (line[0] is not ('2' or '3'))
    {
        throw new IOException($"SMTP command failed: {line}");
    }
}

internal sealed record CreateWidget(string? Name);
internal sealed record MailPreview(string? To);
internal sealed record Widget(long Id, string Name, DateTime CreatedAt);
