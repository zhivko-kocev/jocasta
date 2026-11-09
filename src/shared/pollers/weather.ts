import { type Static, type TSchema, Type } from "typebox";
import { Value } from "typebox/value";
import type { Weather } from "../../frontier/protocol.ts";
import { checkSettings, type Settings } from "../settings.ts";

/**
 * The weather on the dashboard (0429) from Open-Meteo: keyless JSON, free for non-commercial use, CC-BY 4.0. The place
 * is `dashboard.weather` in `jocasta.yaml`: a name ("Example City", "Example City, Example Country") resolved once
 * through Open-Meteo's geocoding, or `latitude, longitude`.
 */

export const GEOCODING_ENDPOINT = "https://geocoding-api.open-meteo.com/v1/search";
export const FORECAST_ENDPOINT = "https://api.open-meteo.com/v1/forecast";

/** A resolved place. */
export interface Place {
	name: string;
	latitude: number;
	longitude: number;
}

/** WMO weather interpretation codes as Open-Meteo documents them, in sentence case. */
const WMO: Record<number, string> = {
	0: "Clear sky",
	1: "Mainly clear",
	2: "Partly cloudy",
	3: "Overcast",
	45: "Fog",
	48: "Rime fog",
	51: "Light drizzle",
	53: "Drizzle",
	55: "Dense drizzle",
	56: "Light freezing drizzle",
	57: "Freezing drizzle",
	61: "Light rain",
	63: "Rain",
	65: "Heavy rain",
	66: "Light freezing rain",
	67: "Freezing rain",
	71: "Light snow",
	73: "Snow",
	75: "Heavy snow",
	77: "Snow grains",
	80: "Light showers",
	81: "Showers",
	82: "Violent showers",
	85: "Light snow showers",
	86: "Snow showers",
	95: "Thunderstorm",
	96: "Thunderstorm with light hail",
	99: "Thunderstorm with hail",
};

function weatherSummary(code: number): string {
	return WMO[code] ?? "Unknown";
}

/**
 * `dashboard.weather` (0429): the place, or undefined when unset. A bad value throws, naming the file, line and key
 * (0584), and turns the weather off.
 */
export function readWeatherPlace(settings: Record<string, unknown>): string | undefined {
	checkSettings(settings, "dashboard.weather");
	return (settings as Settings).dashboard?.weather?.trim() || undefined;
}

/** Fetches one Open-Meteo answer and checks its shape; every failure is one plain line naming the endpoint. */
async function getJson<T extends TSchema>(
	url: string,
	what: string,
	schema: T,
	fetchUrl: typeof fetch,
): Promise<Static<T>> {
	let response: Response;
	try {
		response = await fetchUrl(url, { signal: AbortSignal.timeout(30_000) });
	} catch {
		throw new Error(`Weather: Open-Meteo's ${what} couldn't be reached`);
	}
	if (!response.ok) throw new Error(`Weather: Open-Meteo's ${what} answered HTTP ${response.status}`);
	let body: unknown;
	try {
		body = await response.json();
	} catch {
		throw new Error(`Weather: Open-Meteo's ${what} answer isn't JSON`);
	}
	if (!Value.Check(schema, body)) throw new Error(`Weather: Open-Meteo's ${what} answer has an unknown shape`);
	return body;
}

const GeocodingAnswer = Type.Object({
	results: Type.Optional(
		Type.Array(
			Type.Object({
				name: Type.String(),
				latitude: Type.Number(),
				longitude: Type.Number(),
				country: Type.Optional(Type.String()),
				admin1: Type.Optional(Type.String()),
			}),
		),
	),
});

const ForecastAnswer = Type.Object({
	utc_offset_seconds: Type.Number(),
	current: Type.Object({ time: Type.String(), temperature_2m: Type.Number(), weather_code: Type.Number() }),
	daily: Type.Object({
		temperature_2m_max: Type.Array(Type.Number(), { minItems: 1 }),
		temperature_2m_min: Type.Array(Type.Number(), { minItems: 1 }),
	}),
});

const COORDINATES = /^(-?\d{1,2}(?:\.\d+)?)\s*,\s*(-?\d{1,3}(?:\.\d+)?)$/;

/**
 * Resolves `dashboard.weather`: coordinates as given, else the first geocoding match of the name before the first
 * comma whose country or region matches what follows it ("Example City, Example Country").
 */
export async function findPlace(place: string, fetchUrl: typeof fetch = fetch): Promise<Place> {
	const coordinates = COORDINATES.exec(place);
	if (coordinates) {
		const latitude = Number(coordinates[1]);
		const longitude = Number(coordinates[2]);
		if (Math.abs(latitude) <= 90 && Math.abs(longitude) <= 180) return { name: place, latitude, longitude };
	}
	const [name = "", ...rest] = place.split(",").map((part) => part.trim());
	const qualifier = rest.join(", ").toLowerCase();
	const { results = [] } = await getJson(
		`${GEOCODING_ENDPOINT}?name=${encodeURIComponent(name)}&count=10&language=en&format=json`,
		"geocoding",
		GeocodingAnswer,
		fetchUrl,
	);
	const match = qualifier
		? results.find((r) => [r.country, r.admin1].some((part) => part?.toLowerCase() === qualifier))
		: results[0];
	if (!match) throw new Error(`Weather: Open-Meteo knows no place "${place}"; set dashboard.weather to another name`);
	return {
		name: [match.name, match.country].filter(Boolean).join(", "),
		latitude: match.latitude,
		longitude: match.longitude,
	};
}

/** The weather now and today's high and low at a place (0429). */
export async function pollWeather(place: Place, fetchUrl: typeof fetch = fetch): Promise<Weather> {
	const query = new URLSearchParams({
		latitude: String(place.latitude),
		longitude: String(place.longitude),
		current: "temperature_2m,weather_code",
		daily: "temperature_2m_max,temperature_2m_min",
		timezone: "auto",
		forecast_days: "1",
	});
	const body = await getJson(`${FORECAST_ENDPOINT}?${query}`, "forecast", ForecastAnswer, fetchUrl);
	const code = body.current.weather_code;
	// `current.time` is the place's local time without an offset; `utc_offset_seconds` makes it a moment.
	const local = Date.parse(`${body.current.time}Z`);
	return {
		place: place.name,
		tempC: body.current.temperature_2m,
		code,
		summary: weatherSummary(code),
		highC: body.daily.temperature_2m_max[0] as number,
		lowC: body.daily.temperature_2m_min[0] as number,
		at: Number.isNaN(local) ? "" : new Date(local - body.utc_offset_seconds * 1000).toISOString(),
	};
}
