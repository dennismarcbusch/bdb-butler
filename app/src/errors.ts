import { BackendError } from './api';
import { FormError } from './form';

/** Lesbarer Text für einen Fehler, den die Oberfläche anzeigen kann. */
export function messageOf(error: unknown): string {
  if (error instanceof BackendError || error instanceof FormError) return error.message;
  if (error instanceof Error) return error.message;
  return 'Unbekannter Fehler';
}
