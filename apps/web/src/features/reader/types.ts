import type { Book, ReadingProgress } from "@autolibre/shared";

export type ReaderProgressUpdate = {
  percentage: number;
  cfi?: string | null;
  page?: number | null;
};

export type ReaderComponentProps = {
  book: Book;
  format: string;
  initialProgress: ReadingProgress | null;
  onProgressChange: (progress: ReaderProgressUpdate) => void;
};

export type ComicReaderProps = {
  bookId: string;
  onProgressChange?: (progress: ReaderProgressUpdate) => void;
};
