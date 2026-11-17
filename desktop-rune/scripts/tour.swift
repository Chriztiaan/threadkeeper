import Foundation
import ImageIO
import UniformTypeIdentifiers
let args=CommandLine.arguments
let output=URL(fileURLWithPath:args[1])
let inputs=Array(args.dropFirst(2))
let gif=CGImageDestinationCreateWithURL(output as CFURL, UTType.gif.identifier as CFString, inputs.count, nil)!
CGImageDestinationSetProperties(gif,[kCGImagePropertyGIFDictionary:[kCGImagePropertyGIFLoopCount:0]] as CFDictionary)
for path in inputs {
    let source=CGImageSourceCreateWithURL(URL(fileURLWithPath:path) as CFURL,nil)!
    let frame=CGImageSourceCreateImageAtIndex(source,0,nil)!
    CGImageDestinationAddImage(gif,frame,[kCGImagePropertyGIFDictionary:[kCGImagePropertyGIFDelayTime:3.0]] as CFDictionary)
}
if !CGImageDestinationFinalize(gif){fatalError("Could not write GIF")}
