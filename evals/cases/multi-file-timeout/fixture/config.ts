/** The acme status client's options. */
export interface ClientConfig {
	baseUrl: string;
}

export const DEFAULT_CONFIG: ClientConfig = {
	baseUrl: "https://status.example.org",
};

/** The defaults with `overrides` applied. */
export function makeConfig(overrides: Partial<ClientConfig> = {}): ClientConfig {
	return { ...DEFAULT_CONFIG, ...overrides };
}
