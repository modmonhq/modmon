import { readFile } from 'node:fs/promises';
import { createServer } from 'node:http';

const port = Number.parseInt(process.env.PORT ?? '8080', 10);
const apiUrl = process.env.PUBLIC_API_URL ?? 'http://localhost:3000';
const template = await readFile(new URL('./public/index.html', import.meta.url), 'utf8');
const serializedApiUrl = JSON.stringify(apiUrl).replaceAll('<', '\\u003c');
const page = template.replace('__API_URL__', serializedApiUrl);

const server = createServer((request, response) => {
  const pathname = new URL(request.url ?? '/', 'http://localhost').pathname;

  if (pathname === '/health') {
    response.writeHead(200, { 'content-type': 'application/json; charset=utf-8' });
    response.end(JSON.stringify({ status: 'ok' }));
    return;
  }

  if (pathname === '/' || pathname === '/index.html') {
    response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
    response.end(page);
    return;
  }

  response.writeHead(404, { 'content-type': 'application/json; charset=utf-8' });
  response.end(JSON.stringify({ error: 'not_found' }));
});

server.listen(port, '0.0.0.0', () => {
  console.log(`frontend listening on :${port}`);
});

for (const signal of ['SIGINT', 'SIGTERM']) {
  process.once(signal, () => server.close());
}
