import AppKit
let path = CommandLine.arguments[1]
let size = NSSize(width:1024,height:1024)
let image = NSImage(size:size)
image.lockFocus()
NSColor.clear.setFill();NSRect(origin:.zero,size:size).fill()
let tile=NSBezierPath(roundedRect:NSRect(x:55,y:55,width:914,height:914),xRadius:210,yRadius:210)
let shadow=NSShadow();shadow.shadowColor=NSColor.black.withAlphaComponent(0.18);shadow.shadowBlurRadius=28;shadow.shadowOffset=NSSize(width:0,height:-10);shadow.set()
NSColor(calibratedRed:0.96,green:0.96,blue:0.98,alpha:1).setFill();tile.fill()
NSShadow().set()
let mark = NSImage(contentsOfFile:CommandLine.arguments[2])!
let markSize = mark.size
let width:CGFloat = 690
let height = width * markSize.height / markSize.width
mark.draw(in:NSRect(x:(1024-width)/2,y:(1024-height)/2,width:width,height:height))
image.unlockFocus()
let bitmap=NSBitmapImageRep(data:image.tiffRepresentation!)!
try bitmap.representation(using:.png,properties:[:])!.write(to:URL(fileURLWithPath:path))
