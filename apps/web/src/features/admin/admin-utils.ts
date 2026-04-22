export function formatBytes(sizeBytes: number): string {
  if (!Number.isFinite(sizeBytes) || sizeBytes <= 0) {
    return "0 B";
  }

  const units = ["B", "KB", "MB", "GB", "TB"];
  let size = sizeBytes;
  let index = 0;

  while (size >= 1024 && index < units.length - 1) {
    size /= 1024;
    index += 1;
  }

  const decimals = size >= 10 || index === 0 ? 0 : 1;
  return `${size.toFixed(decimals)} ${units[index]}`;
}

export function formatDateTime(value: string | null | undefined): string {
  if (!value) {
    return "—";
  }

  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return value;
  }

  return parsed.toLocaleString();
}

function formatTimeComponent(value: string): string | null {
  if (!/^\d+$/.test(value)) {
    return null;
  }

  const number = Number(value);
  if (!Number.isInteger(number) || number < 0 || number > 59) {
    return null;
  }

  return number.toString().padStart(2, "0");
}

function formatHourComponent(value: string): string | null {
  if (!/^\d+$/.test(value)) {
    return null;
  }

  const number = Number(value);
  if (!Number.isInteger(number) || number < 0 || number > 23) {
    return null;
  }

  return number.toString().padStart(2, "0");
}

function describeDayOfWeek(value: string): string | null {
  const normalized = value.trim();
  const dayNames = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
  ];

  if (normalized === "1-5") {
    return "weekdays";
  }

  if (!/^\d$/.test(normalized)) {
    return null;
  }

  const index = Number(normalized);
  if (index === 7) {
    return "Sunday";
  }

  return dayNames[index] ?? null;
}

export function describeCronExpression(expression: string): string {
  const parts = expression.trim().split(/\s+/);
  if (parts.length !== 5) {
    return "Enter a standard 5-field cron expression.";
  }

  const [minute, hour, dayOfMonth, month, dayOfWeek] = parts;
  const formattedMinute = formatTimeComponent(minute);
  const formattedHour = formatHourComponent(hour);

  if (!formattedMinute || !formattedHour) {
    return "Scheduled by cron.";
  }

  if (dayOfMonth === "*" && month === "*") {
    if (dayOfWeek === "*") {
      return `Every day at ${formattedHour}:${formattedMinute}`;
    }

    const day = describeDayOfWeek(dayOfWeek);
    if (day === "weekdays") {
      return `Weekdays at ${formattedHour}:${formattedMinute}`;
    }
    if (day) {
      return `Every ${day} at ${formattedHour}:${formattedMinute}`;
    }
  }

  return "Scheduled by cron.";
}
