export interface FsNode {
    name: string;
    path: string;
    type: 'directory' | 'file';
    size: number;
    size_human: string;
    modified_time: string;
    readonly: boolean;
    extension?: string;
    children?: FsNode[];
}
