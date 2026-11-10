/** 通用格式化函数。可复用部分只有 formatDateTime（参照实现 format.ts:20-27）；
 *  其 STATUS_LABELS、QUESTION_TYPE_LABELS、matrixColumns 是问卷业务，不复用。 */

export function formatDateTime(value?: string | null): string {
  if (!value) return "-";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return String(value);
  const pad = (part: number) => String(part).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}
