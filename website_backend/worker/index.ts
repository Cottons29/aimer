import { Container, getRandom } from '@cloudflare/containers';

/** Bindings and variables declared in `wrangler.toml`. */
export interface Env {
	AIMER_API_CONTAINER: DurableObjectNamespace<AimerApiContainer>;
	SERVER_IP: string;
	SERVER_PORT: string;
	SERVER_CORS: string;
}

/**
 * How many interchangeable container instances requests are spread over.
 *
 * The blog API is read-only and stateless, so any instance can answer any
 * request. Keep this at or below `max_instances` in `wrangler.toml`.
 */
const INSTANCE_COUNT = 3;

/** The container running the `website_backend` binary. */
export class AimerApiContainer extends Container<Env> {
	sleepAfter = '10m';
	// `defaultPort` blocks requests until the binary listens on it, so it has
	// to agree with the port the binary is told to bind.
	defaultPort = Number(this.env.SERVER_PORT);
	envVars = {
		SERVER_IP: this.env.SERVER_IP,
		SERVER_PORT: this.env.SERVER_PORT,
		SERVER_CORS: this.env.SERVER_CORS,
	};
}

export default {
	async fetch(request, env) {
		const { pathname } = new URL(request.url);
		if (!pathname.startsWith('/api/')) {
			return Response.json({ error: 'not found' }, { status: 404 });
		}

		const container = await getRandom(env.AIMER_API_CONTAINER, INSTANCE_COUNT);
		return container.fetch(request);
	},
} satisfies ExportedHandler<Env>;
