#include "link.h"

Link::Link(std::string href_,std::string txt_,unsigned int line,unsigned int pos,unsigned int length):_href(href_),_txt(txt_),Token (pos,line,length)
{

}
