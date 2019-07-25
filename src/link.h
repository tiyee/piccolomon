#ifndef LINK_H
#define LINK_H
#include<iostream>
#include"token.h"
class Link : public Token
{
private:
    const std::string _href;
   const  std::string _txt;


public:
    Link(std::string href_,std::string txt_,unsigned int line,unsigned int pos,unsigned int length);
    Link(const Link &) = delete;

    Link&operator=(const Link &) = delete;
    void writeAsHtml(std::ostream&) override;
    void writeToken(std::ostream& out) override;
};

#endif // LINK_H
