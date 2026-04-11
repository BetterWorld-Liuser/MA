import { invoke } from '@tauri-apps/api/core';

export type FrontendDiagnosticLevel = 'debug' | 'info' | 'warn' | 'error';

export type FrontendDiagnosticValue = string | number | boolean | null | undefined;

export type FrontendDiagnosticFields = Record<string, FrontendDiagnosticValue>;

export type FrontendDiagnosticPayload = {
  level: FrontendDiagnosticLevel;
  scope: string;
  event: string;
  message?: string;
  fields?: FrontendDiagnosticFields;
};

export async function writeFrontendDiagnosticLog(
  payload: FrontendDiagnosticPayload,
): Promise<void> {
  await invoke('write_frontend_diagnostic_log', {
    request: {
      level: payload.level,
      scope: payload.scope,
      event: payload.event,
      message: payload.message,
      fields: normalizeFrontendDiagnosticFields(payload.fields),
    },
  });
}

export const frontendDiagnosticLogger = {
  debug(scope: string, event: string, fields?: FrontendDiagnosticFields) {
    return emitFrontendDiagnostic('debug', scope, event, fields);
  },
  info(scope: string, event: string, fields?: FrontendDiagnosticFields) {
    return emitFrontendDiagnostic('info', scope, event, fields);
  },
  warn(scope: string, event: string, fields?: FrontendDiagnosticFields) {
    return emitFrontendDiagnostic('warn', scope, event, fields);
  },
  error(scope: string, event: string, fields?: FrontendDiagnosticFields) {
    return emitFrontendDiagnostic('error', scope, event, fields);
  },
};

async function emitFrontendDiagnostic(
  level: FrontendDiagnosticLevel,
  scope: string,
  event: string,
  fields?: FrontendDiagnosticFields,
) {
  try {
    await writeFrontendDiagnosticLog({
      level,
      scope,
      event,
      fields,
    });
  } catch (error) {
    console.warn('[frontend-diagnostic-log] failed to write diagnostic log', {
      level,
      scope,
      event,
      error,
    });
  }
}

function normalizeFrontendDiagnosticFields(fields?: FrontendDiagnosticFields) {
  if (!fields) {
    return undefined;
  }

  const normalizedEntries = Object.entries(fields)
    .filter(([, value]) => value !== undefined)
    .map(([key, value]) => [key, normalizeFrontendDiagnosticValue(value!)] as const);

  if (normalizedEntries.length === 0) {
    return undefined;
  }

  return Object.fromEntries(normalizedEntries);
}

function normalizeFrontendDiagnosticValue(value: Exclude<FrontendDiagnosticValue, undefined>) {
  if (value === null) {
    return 'null';
  }

  return String(value);
}
