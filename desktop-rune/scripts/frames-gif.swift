import Foundation
import ImageIO
import UniformTypeIdentifiers
let args=CommandLine.arguments
let output=URL(fileURLWithPath:args[1])
let folder=URL(fileURLWithPath:args[2])
let fps=Double(args[3])!
let inputs=try FileManager.default.contentsOfDirectory(at:folder,includingPropertiesForKeys:nil).filter{$0.pathExtension=="png"}.sorted{$0.lastPathComponent<$1.lastPathComponent}
// Coalesce identical frames while preserving their accumulated screen time.
var runs:[(URL,Int)]=[]
var previous:Data?=nil
for path in inputs {
    let bytes=try Data(contentsOf:path)
    if bytes==previous {runs[runs.count-1].1 += 1} else {runs.append((path,1));previous=bytes}
}
let gif=CGImageDestinationCreateWithURL(output as CFURL,UTType.gif.identifier as CFString,runs.count,nil)!
CGImageDestinationSetProperties(gif,[kCGImagePropertyGIFDictionary:[kCGImagePropertyGIFLoopCount:0]] as CFDictionary)
for (path,count) in runs {
    autoreleasepool {
        let source=CGImageSourceCreateWithURL(path as CFURL,nil)!
        let frame=CGImageSourceCreateImageAtIndex(source,0,nil)!
        CGImageDestinationAddImage(gif,frame,[kCGImagePropertyGIFDictionary:[kCGImagePropertyGIFDelayTime:Double(count)/fps]] as CFDictionary)
    }
}
if !CGImageDestinationFinalize(gif){fatalError("Could not write GIF")}
