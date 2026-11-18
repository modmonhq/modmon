import { createServer } from 'node:http';
import { createConnection } from 'node:net';
import { MongoClient } from 'mongodb';

const port = Number.parseInt(process.env.PORT ?? '3000', 10);
const origin = process.env.FRONTEND_ORIGIN ?? '*';
const databaseName = process.env.MONGO_DATABASE ?? 'notes';
const username = encodeURIComponent(process.env.MONGO_USERNAME ?? 'notes');
const password = encodeURIComponent(process.env.MONGO_PASSWORD ?? '');
const host = process.env.MONGO_HOST ?? 'mongo';
const mongoPort = process.env.MONGO_PORT ?? '27017';
const authSource = process.env.MONGO_AUTH_SOURCE ?? 'admin';
const connectionString = `mongodb://${username}:${password}@${host}:${mongoPort}/${databaseName}?authSource=${authSource}`;

const client = new MongoClient(connectionString);
await connectWithRetry(client);
const notes = client.db(databaseName).collection('notes');

const server = createServer(async (request, response) => {
  const pathname = new URL(request.url ?? '/', 'http://localhost').pathname;
  setCors(response, origin);

  if (request.method === 'OPTIONS') {
    response.writeHead(204);
    response.end();
    return;
  }

  try {
    if (request.method === 'GET' && pathname === '/health') {
      await client.db('admin').command({ ping: 1 });
      sendJson(response, 200, { status: 'ok' });
      return;
    }

    if (request.method === 'GET' && pathname === '/api/notes') {
      const result = await notes.find({}, { projection: { text: 1 } })
        .sort({ _id: -1 })
        .limit(50)
        .toArray();
      sendJson(response, 200, result.map(({ _id, text }) => ({ id: _id.toString(), text })));
      return;
    }

    if (request.method === 'POST' && pathname === '/api/notes') {
      const body = await readJson(request);
      const text = typeof body.text === 'string' ? body.text.trim() : '';

      if (text.length < 1 || text.length > 500) {
        sendJson(response, 422, { error: 'text_must_be_between_1_and_500_characters' });
        return;
      }

      const result = await notes.insertOne({ text, createdAt: new Date() });
      sendJson(response, 201, { id: result.insertedId.toString(), text });
      return;
    }

    if (request.method === 'POST' && pathname === '/api/mail-preview') {
      const body = await readJson(request);
      const recipient = typeof body.to === 'string' ? body.to.trim() : '';
      if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(recipient)) {
        sendJson(response, 422, { error: 'valid_recipient_required' });
        return;
      }

      await sendPreviewMail(recipient);
      sendJson(response, 202, {
        accepted: true,
        mailpit: process.env.MM_MAIL_UI_URL ?? null,
      });
      return;
    }

    sendJson(response, 404, { error: 'not_found' });
  } catch (error) {
    if (error instanceof HttpError) {
      sendJson(response, error.status, { error: error.code });
      return;
    }

    console.error(error);
    sendJson(response, 500, { error: 'internal_error' });
  }
});

server.listen(port, '0.0.0.0', () => {
  console.log(`backend listening on :${port}`);
});

let shuttingDown = false;
for (const signal of ['SIGINT', 'SIGTERM']) {
  process.on(signal, async () => {
    if (shuttingDown) {
      return;
    }

    shuttingDown = true;
    server.close();
    await client.close();
  });
}

async function sendPreviewMail(recipient) {
  const host = process.env.MM_MAIL_SMTP_HOST;
  const mailPort = Number.parseInt(process.env.MM_MAIL_SMTP_PORT ?? '1025', 10);
  if (!host) {
    throw new HttpError(503, 'development_mail_not_configured');
  }

  await new Promise((resolve, reject) => {
    const socket = createConnection({ host, port: mailPort });
    let response = '';
    let step = 0;
    const commands = [
      'EHLO backend.node-fullstack.test',
      'MAIL FROM:<preview@node-fullstack.test>',
      `RCPT TO:<${recipient}>`,
      'DATA',
      `Subject: mm Mailpit preview\r\nFrom: preview@node-fullstack.test\r\nTo: ${recipient}\r\n\r\nThe Node example reached shared Mailpit.\r\n.`,
      'QUIT',
    ];

    socket.setEncoding('utf8');
    socket.setTimeout(5_000, () => socket.destroy(new Error('SMTP timeout')));
    socket.on('error', reject);
    socket.on('data', (chunk) => {
      response += chunk;
      const lines = response.split('\r\n');
      response = lines.pop() ?? '';
      for (const line of lines) {
        if (!/^\d{3} /.test(line)) continue;
        if (line.startsWith('4') || line.startsWith('5')) {
          socket.destroy(new Error(`SMTP rejected command: ${line}`));
          return;
        }
        if (step < commands.length) {
          socket.write(`${commands[step]}\r\n`);
          step += 1;
        } else {
          socket.end();
        }
      }
    });
    socket.on('close', (hadError) => {
      if (!hadError && step === commands.length) resolve();
    });
  });
}

async function connectWithRetry(mongoClient) {
  let lastError;

  for (let attempt = 1; attempt <= 20; attempt += 1) {
    try {
      await mongoClient.connect();
      return;
    } catch (error) {
      lastError = error;
      if (attempt < 20) {
        await new Promise((resolve) => setTimeout(resolve, 1_500));
      }
    }
  }

  throw lastError;
}

async function readJson(request) {
  const chunks = [];
  let length = 0;

  for await (const chunk of request) {
    length += chunk.length;
    if (length > 1_048_576) {
      throw new HttpError(413, 'request_body_too_large');
    }
    chunks.push(chunk);
  }

  try {
    return JSON.parse(Buffer.concat(chunks).toString('utf8'));
  } catch {
    throw new HttpError(400, 'invalid_json');
  }
}

function setCors(response, allowedOrigin) {
  response.setHeader('access-control-allow-origin', allowedOrigin);
  response.setHeader('access-control-allow-headers', 'content-type');
  response.setHeader('access-control-allow-methods', 'GET,POST,OPTIONS');
}

function sendJson(response, status, body) {
  response.writeHead(status, { 'content-type': 'application/json; charset=utf-8' });
  response.end(JSON.stringify(body));
}

class HttpError extends Error {
  constructor(status, code) {
    super(code);
    this.status = status;
    this.code = code;
  }
}
