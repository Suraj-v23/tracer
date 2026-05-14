import { describe, it, expect } from 'vitest';
import { formatSize, getFileCategory, getColor, COLORS } from '../utils/utils.js';

describe('formatSize', () => {
    it('returns "0 B" for zero', () => {
        expect(formatSize(0)).toBe('0 B');
    });
    it('formats bytes', () => {
        expect(formatSize(512)).toBe('512 B');
    });
    it('formats kilobytes', () => {
        expect(formatSize(1500)).toBe('1.5 KB');
    });
    it('formats megabytes', () => {
        expect(formatSize(2_500_000)).toBe('2.5 MB');
    });
});

describe('getFileCategory', () => {
    it('returns directory for dir type', () => {
        expect(getFileCategory({ type: 'directory' })).toBe('directory');
    });
    it('identifies image by extension', () => {
        expect(getFileCategory({ type: 'file', extension: '.jpg' })).toBe('image');
    });
    it('identifies code by extension', () => {
        expect(getFileCategory({ type: 'file', extension: '.ts' })).toBe('code');
    });
    it('returns other for unknown extension', () => {
        expect(getFileCategory({ type: 'file', extension: '.xyz' })).toBe('other');
    });
    it('handles missing extension', () => {
        expect(getFileCategory({ type: 'file' })).toBe('other');
    });
});

describe('getColor', () => {
    it('returns folder color for directories', () => {
        expect(getColor({ type: 'directory' })).toBe(COLORS.folder);
    });
    it('returns image color for .png', () => {
        expect(getColor({ type: 'file', extension: '.png' })).toBe(COLORS.image);
    });
    it('returns other color for unknown type', () => {
        expect(getColor({ type: 'file', extension: '.xyz' })).toBe(COLORS.other);
    });
});
