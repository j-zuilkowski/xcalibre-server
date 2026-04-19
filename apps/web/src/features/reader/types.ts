import type { Book, ReadingProgress } from "@calibre/shared";

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
