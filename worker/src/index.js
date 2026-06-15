const ALLOWED_ORIGINS = [
  "https://krankulator.teknodromen.se",
  "https://localhost:8080",
  "https://192.168.68.88:8080",
];

const ALLOWED_HOSTS = [
  "archive.org",
  "ia601604.us.archive.org",
  "ia801604.us.archive.org",
];

function isAllowedUrl(url) {
  try {
    const parsed = new URL(url);
    return ALLOWED_HOSTS.some(
      (h) => parsed.hostname === h || parsed.hostname.endsWith(".archive.org")
    );
  } catch {
    return false;
  }
}

export default {
  async fetch(request) {
    const origin = request.headers.get("Origin") || "";

    if (request.method === "OPTIONS") {
      return new Response(null, {
        headers: {
          "Access-Control-Allow-Origin": origin,
          "Access-Control-Allow-Methods": "GET",
          "Access-Control-Allow-Headers": "Content-Type",
          "Access-Control-Max-Age": "86400",
        },
      });
    }

    const url = new URL(request.url).searchParams.get("url");
    if (!url) {
      return new Response("Missing ?url= parameter", { status: 400 });
    }

    if (!isAllowedUrl(url)) {
      return new Response("URL not allowed", { status: 403 });
    }

    if (!ALLOWED_ORIGINS.some((o) => origin === o)) {
      return new Response("Origin not allowed", { status: 403 });
    }

    const resp = await fetch(url);
    const headers = new Headers(resp.headers);
    headers.set("Access-Control-Allow-Origin", origin);

    return new Response(resp.body, {
      status: resp.status,
      headers,
    });
  },
};
