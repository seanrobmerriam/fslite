import { useState } from "react";

import { MAX_ACTIVITY_RECORDS } from "../../lib/browser/reducer";
import type { ActivityRecord, JsonValue } from "../../lib/shared/contracts";

export interface ApiActivityProps {
  activities: readonly ActivityRecord[];
  onClear(): void;
}

const privateKey =
  /authorization|bearer|token|api[-_]?key|private[-_]?key|cookie|secret|password|server|upstream|internal|headers?/i;
const privateScalar =
  /(?:bearer|basic)\s+\S+|(?:authorization|cookie|token|secret)\s*[:=]|https?:\/\//i;
function sanitize(value: JsonValue | null): JsonValue | null {
  if (typeof value === "string") {
    return privateScalar.test(value) ? "[REDACTED]" : value;
  }
  if (Array.isArray(value)) return value.map(sanitize);
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value).flatMap(([key, child]) =>
        privateKey.test(key) ? [] : [[key, sanitize(child)]],
      ),
    );
  }
  return value;
}
function curlFor(activity: ActivityRecord): string {
  const method = ["GET", "POST", "PUT", "DELETE"].includes(activity.method)
    ? activity.method
    : "GET";
  const path =
    activity.path.startsWith("/") &&
    ![...activity.path].some((character) => {
      const code = character.codePointAt(0) ?? 0;
      return code < 0x20 || code === 0x7f;
    })
      ? activity.path
      : "/";
  return `curl -X ${method} -H 'Authorization: Bearer $FSLITE_TOKEN' '$FSLITE_SERVER_URL${path.replaceAll("'", "'%27")}'`;
}
function bounded(value: JsonValue | null): boolean {
  if (Array.isArray(value)) return value.some(bounded);
  if (value && typeof value === "object")
    return Object.entries(value).some(
      ([key, child]) => /truncated|bounded/i.test(key) || bounded(child),
    );
  return false;
}

/** Browser-local, credential-redacted inspection of visitor initiated upstream work. */
export function ApiActivity({ activities, onClear }: ApiActivityProps) {
  const [message, setMessage] = useState("");
  const visibleActivities = activities.slice(-MAX_ACTIVITY_RECORDS);
  const copy = async (activity: ActivityRecord) => {
    try {
      await globalThis.navigator.clipboard.writeText(curlFor(activity));
      setMessage("Sanitized curl copied.");
    } catch {
      setMessage("Could not copy sanitized curl.");
    }
  };
  return (
    <section className="api-activity" aria-label="API activity">
      <div className="panel-heading">
        <h2>API activity</h2>
        <button
          type="button"
          className="button button--quiet"
          disabled={visibleActivities.length === 0}
          onClick={onClear}
        >
          Clear local activity
        </button>
      </div>
      <p className="activity-note">
        Only your browser’s most recent 100 visitor-initiated requests are kept
        here.
      </p>
      <p className="sr-only" role="status">
        {message}
      </p>
      {visibleActivities.length === 0 ? (
        <p className="panel-empty">No local activity yet.</p>
      ) : (
        <ol className="activity-list">
          {visibleActivities.map((activity) => (
            <li key={activity.id}>
              <details>
                <summary>
                  <span>
                    {activity.method} {activity.path}
                  </span>
                  <span>
                    {activity.status} · {activity.durationMs} ms ·{" "}
                    {activity.requestId}
                  </span>
                </summary>
                {bounded(activity.response) ? (
                  <p className="activity-bounded">
                    Response summary is truncated or bounded.
                  </p>
                ) : null}
                <pre>
                  {JSON.stringify(
                    {
                      request: sanitize(activity.request),
                      response: sanitize(activity.response),
                    },
                    null,
                    2,
                  )}
                </pre>
                <button
                  type="button"
                  className="button button--quiet"
                  onClick={() => void copy(activity)}
                >
                  Copy curl
                </button>
                <pre>{curlFor(activity)}</pre>
              </details>
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}
